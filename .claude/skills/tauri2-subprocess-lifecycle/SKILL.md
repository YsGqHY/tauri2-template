---
name: tauri2-subprocess-lifecycle
description: tauri2-template 的受限子进程与生命周期管理指南。凡是用户要运行外部命令、读取 stdout/stderr、实现停止/取消、进程组终止、输出缓冲或应用退出清理，即使只说“加一个命令执行器”，也应使用此技能。
---

# 受限子进程与生命周期

本项目不把任意 shell 执行暴露给前端。前端只能请求 Rust 侧预先登记的 `CommandSpec`，由专用 worker 独占 `Child`，reader 线程读取输出，Tauri events 将有限数据推送回页面。

---

## 1. 负责范围与真实文件

| 领域 | 文件 | 职责 |
|---|---|---|
| 领域服务 | [src-tauri/src/subprocess.rs](../../../src-tauri/src/subprocess.rs) | 白名单校验、spawn、reader、停止、历史快照 |
| 平台进程组 | [src-tauri/src/process_group.rs](../../../src-tauri/src/process_group.rs) | Unix `setsid`/进程组、Windows JobObject/taskkill fallback |
| 状态 | [src-tauri/src/state.rs](../../../src-tauri/src/state.rs) | active/history、取消标志、worker registry、shutdown gate |
| IPC | [src-tauri/src/commands/subprocess.rs](../../../src-tauri/src/commands/subprocess.rs) | 主窗口 command facade |
| 事件 | [src-tauri/src/events.rs](../../../src-tauri/src/events.rs) | ready/stdout/stderr/exit event |
| 前端 | [src/services/subprocess.ts](../../../src/services/subprocess.ts)、[src/pages/subprocess/index.ts](../../../src/pages/subprocess/index.ts) | typed service、流事件监听、快照恢复、停止按钮 |

不要把 `Command::new` 直接写进页面或普通 command；不要把这个 skill 扩展成任意 shell、stdin、env 或通用 external context 指南，因为当前 contract 没有这些能力。

---

## 2. 请求模型与安全白名单

前端请求只有：

```ts
interface SubprocessRequest {
  command: string;
  args: string[];
  cwd?: string;
}
```

Rust 侧先按 `command` 查找 `CommandSpec`，再依次校验：

1. command ID 必须存在于白名单。
2. 参数数量必须同时满足 `max_args` 和 `arg_patterns.len()`。
3. 每个参数按 `any`、`literal`、`prefix` 或 `regex` 模式匹配。
4. `cwd` 或白名单根目录必须能 canonicalize。
5. 传入 CWD 必须位于 `cwd_root` 之下，防止 `..` 穿越。
6. stdin 固定为 null；stdout/stderr 必须 piped。

失败返回稳定错误码：`COMMAND_NOT_ALLOWED`、`ARGS_NOT_ALLOWED`、`CWD_INVALID`、`CWD_ROOT_INVALID` 或 `CWD_NOT_ALLOWED`。不要把用户输入拼成 shell 字符串，也不要为了方便把 executable、args、cwd 全部开放给前端。

当前 `AppState::new` 将 `command_whitelist` 初始化为空；代码中尚未看到从配置/数据库加载白名单的逻辑。新增命令前必须先确认白名单来源，不能通过默认放行来“修复”空列表。

---

## 3. spawn 与所有权

`subprocess::run` 的所有权边界是刻意设计的：

1. 检查应用是否已进入 shutdown。
2. 读白名单并验证 request。
3. spawn child，配置进程组/job，取出 stdout/stderr。
4. 在 active registry 中登记 `ManagedSubprocess`、取消 flag 和有界 buffer。
5. 为 stdout/stderr 各启动一个 reader，避免单个 noisy stream 填满 pipe 造成死锁。
6. 先发 `subprocess:ready:<id>`，再启动 worker 等待 child。
7. worker 独占 `Child` 和 `ProcessGroup`；command 只复制 `AtomicBool` 并设置取消信号。

不要让 command 持有 `Child` 的共享锁，也不要在 registry、buffer 或 lifecycle mutex 内等待 OS 进程、sleep、终止或 join。

---

## 4. 输出上限与事件契约

当前硬限制：

- 每个 stdout/stderr 快照最多 256 行，新行淘汰最旧行。
- 单行最多 1 MiB，超长行会消费到换行并追加 `… [truncated]`。
- 已退出进程历史最多 64 个 snapshot。
- 前端页面展示还会将合并日志裁剪到最近 200 行。

事件名集中在 [src/shared/events.ts](../../../src/shared/events.ts)：

```text
subprocess:ready:<id>
subprocess:stdout:<id>
subprocess:stderr:<id>
subprocess:exit:<id>
```

事件顺序契约：`ready` 必须先于 `exit`；stdout/stderr 是两条独立流，不保证跨流时间顺序。worker 退出时会先保存终态历史，再 emit exit；前端应先建立监听，再拉取 bounded snapshot，避免极快退出造成事件丢失。

前端用 [src/pages/subprocess/index.ts](../../../src/pages/subprocess/index.ts) 的模式：订阅 stdout/stderr/exit → `get_subprocess_output` 恢复快照 → 用 `disposed`、pending set 和 unlisten 防止页面卸载后的异步回写。

---

## 5. 停止、强杀与 reap

停止是协作取消，不是 command 线程直接杀进程：

1. `stop_subprocess` 只复制对应的 `Arc<AtomicBool>` 并置为 true。
2. worker 发现取消或全局 shutdown 后，向整个进程组/job 发优雅终止。
3. 默认等待 5 秒 grace period。
4. 超时后强杀整个组/job，并最多等待 2 秒观察 direct child 退出。
5. 最后调用 `Child::kill` + `wait` 兜底，确保 reap，避免 Unix zombie。
6. reader shutdown 最多等待 2 秒；仍被继承 pipe 卡住时记录 detach warning，不能让应用退出无限期等待。

重复 stop、对不存在进程 stop、对历史进程 stop 都是幂等 no-op。应用退出时：

- 先关闭 lifecycle gate，拒绝迟到的 worker 注册。
- 设置全局 stop flag 和所有 active process cancel flag。
- 取出 JoinHandle 列表，释放锁后逐个 join。
- 注册新 worker 时顺便清理已完成的 handle。

这些步骤是可观察契约，不能为了“简单”改成丢弃 JoinHandle 或无期限 join。

---

## 6. 平台差异

| 平台 | 优雅停止 | 强制终止 | 已知风险 |
|---|---|---|---|
| Unix | `setsid` 后向负 PGID 发 SIGTERM | 向 PGID 发 SIGKILL，必要时 direct child kill | 当前仓库尚未做真实 Unix runtime smoke test |
| Windows | `taskkill /T` | JobObject `TerminateJobObject`，失败时 `taskkill /T /F` 或 child kill | JobObject 是 spawn 后绑定，存在短暂逃逸竞态 |
| 其他 | direct child fallback | direct child fallback | 不应宣称有进程树保证 |

不要把 `cargo check --all-targets` 当成跨平台进程行为验证。Windows 和 Unix 都要跑真实长命令、子孙进程、取消、输出收尾和应用退出测试。

---

## 7. 测试与验证

在 `src-tauri/` 下至少运行：

```bash
cargo test subprocess
cargo test state
cargo fmt --check
cargo check --all-targets
```

应覆盖：

- 不在白名单的命令、参数数量和模式不匹配、CWD 越界都被拒。
- 单行超限仍能读取下一行；buffer 达到容量时淘汰最旧行。
- `ready` 先于 `exit`。
- cancel 在 grace + force window 内结束并 reap child。
- shutdown gate 拒绝迟到 worker，finished handle 会被 pruning。
- Windows JobObject attach 失败时 fallback 仍有可观察错误/退出路径。

---

## 8. 反例

❌ `Command::new(request.executable)`，不经过 command whitelist。

❌ 把 args 拼成 `sh -c` 或 `cmd /C` 字符串，导致参数注入和跨平台语义漂移。

❌ 只读 stdout、不读 stderr，子进程可能因 stderr pipe 满而死锁。

❌ 无上限地把每行输出或历史快照放进内存。

❌ 在 command 里直接 `child.kill()`，绕过 worker 的优雅停止、进程树和 reap 协议。

❌ 把 reader `JoinHandle` 丢掉并假设应用退出时它一定会结束。

❌ 声称当前实现支持 stdin、env、调用方 context 或跨流精确时间排序。

---

## 9. 与其他层协调

- command 注册、事件 payload 和 `invokeCommand` 规则见 [tauri2-ipc](../tauri2-ipc/SKILL.md)。
- thread、锁、取消、阻塞 I/O 的总原则见 [tauri2-rust-backend](../tauri2-rust-backend/SKILL.md)。
- 构建与跨平台 smoke-test 边界见 [tauri2-build-release](../tauri2-build-release/SKILL.md)。
- subprocess 页面 mount/cleanup 属于 [tauri2-vanilla-app-architecture](../tauri2-vanilla-app-architecture/SKILL.md)。
