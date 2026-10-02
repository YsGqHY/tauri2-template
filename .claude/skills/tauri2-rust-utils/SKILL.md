---
name: tauri2-rust-utils
description: tauri2-template 的 Rust 内部基础设施指南。凡是用户要新增或修改 AES-GCM 加密、限长文件读写、原子替换、HTTP 超时/重试/取消、结构化日志或日志轮转，即使只说“加一个安全存储 helper”，也应使用此技能。
---

# Rust 内部工具层

`src-tauri/src/utils/` 是给 Rust 业务服务复用的内部库，不是自动暴露给前端的万能 API。每个 helper 都有明确的大小、超时、错误和敏感信息边界。

---

## 1. 负责范围与真实文件

| 模块 | 文件 | 实际能力 |
|---|---|---|
| 加密 | [src-tauri/src/utils/cryptox.rs](../../../src-tauri/src/utils/cryptox.rs) | per-user-data-dir AES-256-GCM、key 文件和 Unix 权限 |
| 文件 | [src-tauri/src/utils/filex.rs](../../../src-tauri/src/utils/filex.rs) | 限长读取、同目录临时文件、sync、原子替换 |
| HTTP | [src-tauri/src/utils/httpx.rs](../../../src-tauri/src/utils/httpx.rs) | shared reqwest blocking client、响应上限、幂等重试、取消检查 |
| 日志 | [src-tauri/src/utils/logx.rs](../../../src-tauri/src/utils/logx.rs) | JSONL、size rotation、备份、敏感标签脱敏 |
| 模块入口 | [src-tauri/src/utils/mod.rs](../../../src-tauri/src/utils/mod.rs) | 暴露内部模块，不注册 command |

新增 helper 时先判断它属于哪种资源边界；不要把多个模块拼成一个“万能工具”。

---

## 2. cryptox：本地 AES-256-GCM

[cryptox.rs](../../../src-tauri/src/utils/cryptox.rs) 的格式是：

```text
base64(NFCR magic + version(1) + 12-byte nonce + AES-GCM ciphertext/tag)
```

行为：

- key 为应用数据目录下 `.master.key` 的 32 个随机字节。
- 缺失 key 使用 `create_new` 创建并 sync；已有 key 必须是非 symlink、长度恰好 32 的普通文件。
- 每次加密随机生成 12-byte nonce。
- 解密检查 base64、magic、version、最小 tag 长度和认证标签。
- 空 encoded payload 解密为空字节。
- Unix 尝试将 data directory 设为 `0700`、key 设为 `0600`；Windows 不管理 ACL。
- 路径组件和 key 文件拒绝 symlink，避免把 key 写到意外位置。

错误不能包含 plaintext、ciphertext、key、URL 或敏感路径。这个设计只能防止普通数据库复制，不能抵抗本机管理员；不要把它描述成 OS keychain 或 Windows ACL 等价物。

使用时：

1. 传入应用自己的 user-data 目录，不要接受前端任意目录。
2. 不打印加密前后的内容。
3. key 创建失败时删除未完成 key，避免留下半个文件。
4. 需要更换格式时增加 version 分支并保留旧版本迁移策略。

---

## 3. filex：有限和原子文件 I/O

`read_limit(path, max_bytes)` 先检查 metadata，再用 `take(max_bytes + 1)` 读取，处理并发增长，超限返回 `InvalidData`。调用方必须给出业务上合理的上限，不要用 `u64::MAX` 伪装成“无限”。

`write_atomic(path, bytes)` 的流程：

1. 校验目标确实命名了文件，并创建父目录。
2. 在目标同目录创建带进程 ID/递增序列的 `create_new` 临时文件。
3. 写入并 `sync_all`。
4. Unix 用 `rename`，Windows 用 `MoveFileExW(REPLACE_EXISTING | WRITE_THROUGH)` 替换。
5. 成功后尽力 sync 父目录；失败时 `Drop` 清理临时文件。

同目录临时文件很重要：跨文件系统 rename 会破坏原子性。当前 helper 不保留既有目标权限；如果业务需要特殊权限，写入后显式设置。不要把 `write_atomic` 与数据库 WAL 快照混用，数据库切换应使用 [tauri2-storage-sqlite](../tauri2-storage-sqlite/SKILL.md) 的专用流程。

---

## 4. httpx：有界 blocking HTTP

`HttpClient` 复用 reqwest 连接池，默认：

- `reqwest 0.12` + rustls
- 请求 timeout 30 秒
- response body 上限 8 MiB
- user-agent `tauri2-template/0.1`
- 默认不重试

自动重试规则：

- 只有 GET、HEAD、OPTIONS、PUT、DELETE 等幂等 method 才会重试。
- POST 默认永不重试，即使设置了 `max_retries`。
- 可重试状态为 408、425、429 和 5xx；transport error 可重试。
- `max_retries` 表示初始请求之后的次数，不是总 attempts。
- delay 按指数退避并封顶；当前实现传入固定 jitter sample `1.0`，不是随机抖动。
- cancellation 在请求前和退避切片之间检查；已经开始的 blocking socket 不能被 token 立即打断，只受 timeout 限制。
- response 读取超过上限返回 `ResponseTooLarge`，不分配无界 body。
- public error 不包含 URL query、response body 或 reqwest 原始 URL-bearing 文本。

从 async command 调用时必须放进 `spawn_blocking` 或专用 worker；不能直接在 async runtime 上执行 `reqwest::blocking`。需要真正可中断的网络请求时，应改用合适的 async client/transport，而不是声称 `CancellationToken` 会中断 socket。

不要把这个内部 client 直接绑定成“前端输入 URL 的任意 fetch command”；若业务要访问远程服务，应增加明确的 allowlist、域名、认证和响应契约。

---

## 5. logx：结构化 JSONL 与轮转

`LogRecord::new` 生成 `unixMs`、大写 `level`、`component` 和 `message`。构造时会对常见敏感标签做整段替换，包括 `api_key`、`access_token`、`password`、`credential`、`.master.key`、`token` 等。

`RotatingFileSink`：

- 默认写 `data_dir/logs/app.log`。
- 默认 8 MiB，保留 3 个备份：`app.log.1` … `app.log.3`。
- 用 `Mutex` 串行写入，JSONL 每条一行并 flush。
- 轮转前 flush + sync_data，再倒序改名。
- Unix 新日志文件尝试使用 `0600`。
- sink 创建/写入失败时可用 `StderrSink` 降级。
- `init` 只构造 sink，不安装 global logger，不改变主线程。

脱敏是标签规则，不是完整秘密扫描：原始 token 没有敏感标签时仍可能被记录。日志消息不应承载密钥、完整 authorization header、密码或完整用户凭据；调用方要在构造记录前做业务级清洗。

---

## 6. 错误与测试

工具层错误应稳定、少泄漏：

- cryptox：`InvalidInput`、`InvalidKey`、`InvalidCiphertext`、`UnsupportedVersion` 等。
- httpx：`InvalidUrl`、`Cancelled`、`Status`、`ResponseTooLarge`、`Transport` 等。
- filex/logx：使用 `io::Error`，由上层转换成可序列化 `AppError`。

已有单测覆盖：

```bash
cd src-tauri
cargo test cryptox
cargo test filex
cargo test httpx
cargo test logx
cargo fmt --check
```

新增测试至少验证：密文 round-trip 和篡改拒绝、读取/响应上限、原子替换和父目录创建、幂等 method 重试而 POST 不重试、取消在 backoff 生效、日志 JSON shape/脱敏/备份数量/并发写入。

---

## 7. 反例

❌ 将 `.master.key` 打入日志、错误 detail 或前端响应。

❌ 用固定 nonce、可预测 key 或把 key 与 ciphertext 放在同一公开配置中。

❌ 用 `fs::write` 直接覆盖关键配置，进程崩溃时留下半文件。

❌ 对 POST 自动重试，可能造成重复副作用。

❌ 让 response body、日志文件或单行日志无上限增长。

❌ 在 async command 中直接调用 `reqwest::blocking`、长时间同步文件 I/O 或 SQLite。

❌ 看到“有脱敏函数”就认为所有秘密都能自动识别。

---

## 8. 与其他层协调

- `AppError`、command 返回错误和序列化见 [tauri2-rust-backend](../tauri2-rust-backend/SKILL.md) 与 [tauri2-ipc](../tauri2-ipc/SKILL.md)。
- 数据库路径切换的 snapshot/rollback 见 [tauri2-storage-sqlite](../tauri2-storage-sqlite/SKILL.md)。
- 阻塞 worker、取消和锁边界见 [tauri2-subprocess-lifecycle](../tauri2-subprocess-lifecycle/SKILL.md)。
- 构建和跨平台文件/进程验证见 [tauri2-build-release](../tauri2-build-release/SKILL.md)。
