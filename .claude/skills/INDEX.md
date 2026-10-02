# tauri2-template Skills 索引

本项目附带的开发技能库按**基础 Tauri 组件**与**Foundation Desktop 领域能力**拆分。调用方式：在对话中直接使用 `/<skill-name>`，或由 AI 根据任务自动匹配加载。

## 基础 Tauri skill

| Skill | 触发场景 | 主要内容 |
|-------|---------|---------|
| [tauri2-rust-backend](./tauri2-rust-backend/SKILL.md) | 写 Rust command、State、Builder、模块或后台 worker | `lib.rs`/`main.rs` 分工、commands/models/state/events、错误、async、锁和线程所有权 |
| [tauri2-frontend](./tauri2-frontend/SKILL.md) | 改 Vanilla TS、Vite、strict 或静态资源 | Vite 端口、strict 陷阱、DOM 基础规则、typed facade 入口 |
| [tauri2-ipc](./tauri2-ipc/SKILL.md) | 前后端 command、event、类型或错误契约 | `invokeCommand`、camelCase↔snake_case、serde 返回值、event 命名和 unlisten |
| [tauri2-config-permissions](./tauri2-config-permissions/SKILL.md) | 改窗口配置、CSP、capability、插件权限或 identifier | `tauri.conf.json`、主/子窗口 ACL、dialog/window 权限、CSP 与 global API |
| [tauri2-build-release](./tauri2-build-release/SKILL.md) | 构建、打包、工具链、跨平台或原生 smoke test | dev/build 链路、Windows 工具链、release profile、打包和验证边界 |

## Foundation Desktop 领域 skill

| Skill | 触发场景 | 主要内容 |
|-------|---------|---------|
| [tauri2-vanilla-app-architecture](./tauri2-vanilla-app-architecture/SKILL.md) | 新增页面、Shell、Router、Store、service、主题、i18n 或页面生命周期 | AppShell、key-based Router、keepAlive、mount/cleanup、异步竞态、theme/i18n、runtime fallback |
| [tauri2-desktop-integration](./tauri2-desktop-integration/SKILL.md) | 子窗口、托盘、原生 dialog、自绘标题栏、窗口控制或跨窗口消息 | child window 生命周期、消息授权、tray action、dialog facade、window capability |
| [tauri2-storage-sqlite](./tauri2-storage-sqlite/SKILL.md) | SQLite schema、迁移、设置、偏好、数据库路径切换、统计、清理或回滚 | WAL、integrity check、schema v2、snapshot/rollback、overwrite 确认、阻塞 I/O |
| [tauri2-subprocess-lifecycle](./tauri2-subprocess-lifecycle/SKILL.md) | 白名单外部命令、stdout/stderr、停止取消、进程组或退出清理 | 参数/CWD 校验、bounded reader、事件顺序、优雅停止、强杀/reap、JoinHandle |
| [tauri2-rust-utils](./tauri2-rust-utils/SKILL.md) | AES-GCM、限长文件、原子替换、HTTP 重试/取消、JSONL 日志或轮转 | cryptox/filex/httpx/logx 的真实限制、错误脱敏、内部工具边界 |

领域 skill 不重复完整的基础 IPC、ACL、Vite 或构建教程；需要跨层时通过上表中的关联 skill 协作。X-Pro DataGrid/chart 是演示页面，不单独建立 skill。

## 组件与 SKILL 对应表

| 项目路径 | 归属 SKILL |
|---------|-----------|
| `src-tauri/src/lib.rs`、`src-tauri/src/main.rs`、`src-tauri/src/commands/`、`src-tauri/src/models/` | tauri2-rust-backend / tauri2-ipc |
| `src-tauri/src/state.rs`、`src-tauri/src/events.rs` | tauri2-rust-backend / tauri2-ipc |
| `src-tauri/src/storage/`、`src-tauri/src/commands/storage.rs`、`src-tauri/src/commands/settings.rs` | tauri2-storage-sqlite |
| `src-tauri/src/subprocess.rs`、`src-tauri/src/process_group.rs`、`src-tauri/src/commands/subprocess.rs` | tauri2-subprocess-lifecycle |
| `src-tauri/src/child_windows.rs`、`src-tauri/src/tray.rs`、`src-tauri/src/commands/windows.rs` | tauri2-desktop-integration |
| `src-tauri/src/utils/` | tauri2-rust-utils |
| `src-tauri/tauri.conf.json`、`src-tauri/capabilities/*.json` | tauri2-config-permissions / tauri2-desktop-integration |
| `index.html`、`src/main.ts`、`src/styles.css`、`src/api/`、`src/contracts/` | tauri2-frontend / tauri2-ipc / tauri2-vanilla-app-architecture |
| `src/components/`、`src/router/`、`src/store/`、`src/theme/`、`src/i18n/`、`src/pages/`、`src/services/` | tauri2-vanilla-app-architecture |
| `package.json`、`vite.config.ts`、`tsconfig.json` | tauri2-frontend / tauri2-build-release |
| `Cargo.toml`、`Cargo.lock`、构建产物与安装包 | tauri2-rust-backend / tauri2-build-release |

## 内容来源

基础 Tauri skill 的入口说明基于本仓库真实代码与实测结果；对应官方 Tauri v2 原文保留在各自 `references/` 目录。新增领域 skill 的 `references/INDEX.md` 只登记本仓库实现和关联基础 skill，不重复复制官方文档。

官方文档同步基准：

- 仓库：`tauri-apps/tauri-docs`
- 分支：`v2`
- commit：`fb135dc6f6894c62ba41a04128e9cb05564a9b7a`
- 同步日期：2026-09-30
- 上游仓库许可证：MIT

基础组件 references 索引：

- [Rust 后端 references](./tauri2-rust-backend/references/INDEX.md)
- [前端 references](./tauri2-frontend/references/INDEX.md)
- [IPC references](./tauri2-ipc/references/INDEX.md)
- [配置与权限 references](./tauri2-config-permissions/references/INDEX.md)
- [构建与发布 references](./tauri2-build-release/references/INDEX.md)

领域实现 references：

- [Vanilla 应用架构 references](./tauri2-vanilla-app-architecture/references/INDEX.md)
- [桌面集成 references](./tauri2-desktop-integration/references/INDEX.md)
- [SQLite 存储 references](./tauri2-storage-sqlite/references/INDEX.md)
- [子进程生命周期 references](./tauri2-subprocess-lifecycle/references/INDEX.md)
- [Rust 工具层 references](./tauri2-rust-utils/references/INDEX.md)

## 版本基准

- Tauri 2.12.0 / tauri-build 2.7.0 / tauri-plugin-opener 2.7.0
- Rust 1.98.1（stable-x86_64-pc-windows-msvc）
- TypeScript 7.0.2 / Vite 8.3.1 / @tauri-apps/api 2.12.0
- Foundation Desktop 迁移基准：2026-10-01
- 基础文档同步基准：2026-09-30
