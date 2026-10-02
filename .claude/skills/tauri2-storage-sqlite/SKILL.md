---
name: tauri2-storage-sqlite
description: tauri2-template 的 SQLite 持久化与数据迁移指南。凡是用户要新增或修改 SQLite schema、迁移、设置/偏好持久化、数据库路径切换、统计、清理或回滚，即使只说“换数据库位置”或“保存设置”，也应使用此技能。
---

# SQLite 存储与数据持久化

本项目的持久化不是 webview `localStorage`，而是 Rust 侧 rusqlite bundled SQLite。前端只通过 typed command facade 读写；数据库连接由 `AppState` 统一持有。

---

## 1. 负责范围与真实文件

| 领域 | 文件 | 职责 |
|---|---|---|
| 初始化与连接 | [src-tauri/src/storage/mod.rs](../../../src-tauri/src/storage/mod.rs) | 默认路径、配置文件、SQLite 连接、WAL、完整性检查 |
| schema | [src-tauri/src/storage/schema.rs](../../../src-tauri/src/storage/schema.rs) | 版本检查与迁移，当前 schema version 为 2 |
| 业务数据 | [src-tauri/src/storage/data.rs](../../../src-tauri/src/storage/data.rs) | 设置、偏好、主题校验、可清理表 |
| 路径切换 | [src-tauri/src/storage/paths.rs](../../../src-tauri/src/storage/paths.rs) | snapshot、安装、配置原子替换、回滚 |
| 统计 | [src-tauri/src/storage/stats.rs](../../../src-tauri/src/storage/stats.rs) | 数据库大小与表行数统计 |
| IPC | [src-tauri/src/commands/settings.rs](../../../src-tauri/src/commands/settings.rs)、[src-tauri/src/commands/storage.rs](../../../src-tauri/src/commands/storage.rs) | 主窗口校验、参数转换、错误返回 |
| 前端 | [src/services/storage.ts](../../../src/services/storage.ts)、[src/pages/settings/index.ts](../../../src/pages/settings/index.ts) | typed facade、覆盖确认、设置页展示 |

不要把 schema、文件替换和事务逻辑塞回 command；command 只做窗口校验、参数转换和调用领域函数。

---

## 2. 初始化顺序

`storage::initialize(default_dir)` 的顺序决定了失败时是否会误写数据库：

1. 创建并 canonicalize 默认数据目录。
2. 读取 `storage.json`，得到可选的 `customPath`。
3. 解析当前数据库绝对路径；无自定义路径时使用 `app.sqlite3`。
4. 打开 SQLite 连接。
5. 先检查 schema version，拒绝未来版本。
6. 执行 `PRAGMA integrity_check`，非 `ok` 时返回 `STORAGE_CORRUPT`。
7. 配置 `busy_timeout=5s`、`journal_mode=WAL`、`foreign_keys=ON`、`synchronous=NORMAL`。
8. 执行迁移并写入默认 `app_config` 行。

未来 schema 必须在任何写入 pragma、建表或迁移前被拒绝。不要为了“自动修复”删除未知版本数据库。

```rust
let connection = Connection::open(path)?;
schema::check_version(&connection)?;
integrity_check(&connection)?;
configure_connection(&connection)?;
migrate(&connection)?;
```

共享连接的实际类型是 `Arc<Mutex<rusqlite::Connection>>`，外层 `StorageState` 由 `AppState.storage: RwLock<_>` 保护。取用数据后尽快释放锁，不要持有数据库 guard 跨异步边界。

---

## 3. schema 与业务数据

当前 schema 在 [src-tauri/src/storage/schema.rs](../../../src-tauri/src/storage/schema.rs)：

- `schema_meta`：保存字符串形式的 version。
- `app_config`：单行 `id=1`，保存 `theme_choice`、`custom_theme_json`、`locale_choice` 和 `updated_at`。
- `user_preferences`：键值 JSON，当前支持 `showLogo`、`showTooltip`。
- version 1 → 2 会补 `updated_at`，并将历史 `locale_choice='system'` 归一为 `auto`。

新增字段时：

1. 提高 `SCHEMA_VERSION`。
2. 在事务内增加从旧版本到新版本的迁移分支。
3. 保证迁移可以在空库、旧库和已有数据上运行。
4. 用测试验证默认值、`updated_at`、旧值归一化和未来版本拒绝。
5. 不要通过 `DROP TABLE` 绕过迁移；需要破坏性变化时先设计备份/回滚。

设置写入走 `update_settings`：读取当前值 → 在事务内修改 → 完整校验 → 写入 → 重新读取 → commit。主题 choice、locale、custom palette 和偏好值都在 Rust 侧再次校验，不能只信前端输入。

---

## 4. 数据库路径切换

路径切换是一个可回滚的文件安装流程，不是简单的 `Connection::open(new_path)`：

1. 将用户路径解析成绝对数据库文件路径；目录输入会补 `app.sqlite3`。
2. 如果目标就是当前路径，只更新配置并返回统计。
3. 目标已存在且 `overwrite=false` 时返回 `STORAGE_TARGET_EXISTS`，绝不改动目标。
4. 校验旧配置，锁定当前连接，执行 WAL checkpoint。
5. 用 rusqlite `Backup` 将当前库 snapshot 到目标目录的临时文件。
6. 对 snapshot 做 `integrity_check`、sync；目标存在时先保存原始字节用于回滚。
7. 以 `persist_noclobber` 安装 snapshot。
8. 打开并迁移新目标数据库。
9. 原子写入 `storage.json`；任何失败都恢复原目标和内存状态。
10. 只有全部成功后，替换 `StorageState.db/current_path/is_custom`。

必须保留以下保护：

- 覆盖目标前显式确认；UI 不能默认传 `overwrite=true`。
- 目标存在 `-wal` 或 `-shm` 时视为正在使用，返回 `STORAGE_BUSY`。
- rollback 只能删除本次明确安装的目标，不能删除任意固定临时文件。
- Windows 下替换文件前释放新连接，否则文件句柄会阻止回滚。
- 配置文件使用同目录临时文件、sync、替换和失败恢复，不能直接 truncate 原文件。

前端流程见 [src/pages/settings/index.ts](../../../src/pages/settings/index.ts)：先调用不覆盖版本；捕获 `STORAGE_TARGET_EXISTS` 后通过 `NativeDialogs.confirm` 再重试 `overwrite=true`。不要把所有错误都转换成覆盖确认。

---

## 5. 统计与清理

`get_storage_stats` 返回当前路径、是否自定义、默认路径和文件大小；大小包含主数据库、`-wal`、`-shm`。表统计返回行数，当前 `sizeBytes=0` 且 `estimated=true` 表示没有伪造精确表大小。

`clear_table` 必须经过 `CLEARABLE_TABLES` 白名单。当前只有 `user_preferences` 可清理，未知表返回 `TABLE_NOT_CLEARABLE`。不要接受前端任意 SQL 表名、列名或 SQL 片段。

---

## 6. command 与异步边界

当前 storage/settings command 标记了 `#[tauri::command(async)]`，但内部仍执行同步 SQLite、文件和 backup 操作。扩展这些 command 时必须重新评估 runtime 阻塞问题：

- 有界且短小的同步操作可保留同步 command。
- 可能较慢的数据库/文件工作放进 `tauri::async_runtime::spawn_blocking`，或交给受管理 worker。
- 不要在持有 `RwLockReadGuard`、`MutexGuard` 时 `.await`。
- 切换数据库前复制所需路径并释放不必要的锁；不能在锁内等待外部 I/O。

通用 async、State 和错误规则见 [tauri2-rust-backend](../tauri2-rust-backend/SKILL.md)；前后端字段与错误 shape 见 [tauri2-ipc](../tauri2-ipc/SKILL.md)。

---

## 7. 测试与验证

在 `src-tauri/` 下运行：

```bash
cargo test storage
cargo fmt --check
cargo check --all-targets
```

至少覆盖：

- 空库迁移后有默认设置和 `updated_at`。
- 未来 schema 在写入前被拒绝。
- 旧 locale 能归一化，设置可重启后持久化。
- 切换到自定义路径后再切回，数据仍保留。
- 目标已存在时无确认不修改原始字节。
- 配置写入失败时目标文件恢复，内存状态不切换。
- WAL/SHM 忙碌目标被拒绝。

业务测试不等于跨平台验证；Windows 文件句柄、Unix 权限和真实运行时的 SQLite lock 仍需在目标平台 smoke test。

---

## 8. 反例

❌ 直接 `Connection::open` 新路径后替换状态，失败时没有 snapshot 和 rollback。

❌ 前端把 `overwrite: true` 固定传入，跳过用户确认。

❌ 只在前端校验主题色或 locale，Rust 侧直接写入任意 JSON。

❌ 允许前端传任意表名并拼接 SQL。

❌ 通过 `fs::copy` 复制正在使用的 WAL 数据库，忽略 checkpoint 和 `-wal/-shm`。

❌ 在 async command 中持有数据库锁做长时间文件操作或 `.await`。

---

## 9. 与其他层协调

- command 注册、统一错误和 camelCase 契约见 [tauri2-ipc](../tauri2-ipc/SKILL.md)。
- `AppState`、锁和阻塞 I/O 规则见 [tauri2-rust-backend](../tauri2-rust-backend/SKILL.md)。
- 设置页的 Router、页面 cleanup、主题和 i18n 组合见 [tauri2-vanilla-app-architecture](../tauri2-vanilla-app-architecture/SKILL.md)。
- 原子文件写入的通用 helper 见 [tauri2-rust-utils](../tauri2-rust-utils/SKILL.md)。
