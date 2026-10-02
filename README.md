# Foundation Desktop — Tauri v2 + Rust + Vanilla TS

本项目是一个 Rust-first 的 Tauri v2 桌面应用脚手架：保留 foundation 的两栏工作台、主题、i18n、设置、SQLite、子窗口、托盘和受限子进程能力，但前端改为无框架 Vanilla TypeScript + Vite，后端改为 Rust typed commands/services。

## 技术栈

- Tauri 2.12.0 / Rust 1.98.1 MSVC
- Vanilla TypeScript 7 + Vite 8
- rusqlite bundled SQLite（新数据模型，不直接兼容 foundation.db）
- Tauri dialog plugin；capabilities 按主窗口/子窗口隔离
- 本地单用户应用，不包含登录、JWT、session 或远程鉴权

## 运行

```bash
pnpm install
pnpm tauri dev
```

仅构建前端：

```bash
pnpm build
```

生产构建和 Windows 安装包：

```bash
pnpm tauri build
```

Rust 校验：

```bash
cd src-tauri
cargo fmt --check
cargo test
cargo check --all-targets
cargo build
```

## 并发与跨平台验证

Foundation 的 goroutine/context/WaitGroup 在 Rust 中按任务所有权改用受管理的 `std::thread` / `JoinHandle`、`AtomicBool` 协作停止标志与 `Mutex` / `RwLock` 状态保护；Tauri events 承担前端事件通知。rusqlite 共用连接由 `Arc<Mutex<Connection>>` 串行访问，子进程输出快照每流保留最多 256 行；单行最多 1 MiB，超限追加截断标记；退出历史最多 64 项。生命周期 gate 会拒绝 shutdown 后的迟到注册，`ready` 事件先于 worker 发出，完成的 `JoinHandle` 会被清理。

异步 command 不应直接做阻塞 I/O（包括 `reqwest::blocking` 和同步文件/数据库长操作）；按工作类型选择 `spawn_blocking`、可回收的专用线程、经测量后用于 CPU 密集工作的 Rayon，或受白名单限制的子进程。锁 guard 不得跨 `.await`。当前子进程 worker 有 5 秒优雅停止期与 2 秒强杀等待上限，强杀后会 reap child；reader shutdown 最多等待 2 秒，超时发出最后手段 detach warning。Windows 使用 JobObject（spawn 后绑定，存在竞态并保留 taskkill fallback），Unix 使用进程组 SIGTERM/SIGKILL；shutdown lifecycle gate 拒绝迟到注册，并清理已完成的 `JoinHandle`。Unix runtime 尚未验证。Foundation 的通用 external context、`KillGracePeriod`、stdin、env、`CaptureOutput` API 未暴露为当前 Tauri contract。见 [CLAUDE.md](CLAUDE.md) 及 [子进程实现](src-tauri/src/subprocess.rs#L190-L237)、[平台进程组](src-tauri/src/process_group.rs#L105-L162)、[退出清理](src-tauri/src/state.rs#L82-L108)。

`cargo test`、`cargo check --all-targets` 和 `cargo build` 都只覆盖当前目标平台/工具链；`cargo check` 不做最终链接。进程树停止、取消时输出收尾及应用退出等待应在 Windows 与 Unix 类平台分别实测。本次文档更新未执行这些代码验证；此前 Windows 构建/打包记录不代表 Unix/macOS 已验证。

## 主要功能

- 自绘标题栏、Sidebar、key-based 路由、启动骨架
- Home：Rust greet IPC、`app:time` 事件、子窗口演示
- Settings：主题/自定义色板、语言、数据库路径与表统计
- i18n：`zh-CN` / `en-US` / `auto`
- SQLite：版本化 schema、WAL、完整性检查、路径切换与回滚
- Native dialog、tray、child windows、白名单 subprocess、X-Pro 轻量 demo

## 目录

- `src/`：Vanilla TS 组件、页面、services、contracts、theme、i18n
- `src-tauri/src/commands/`：Tauri IPC command
- `src-tauri/src/storage/`：SQLite、迁移、统计和路径切换
- `src-tauri/src/utils/`：HTTP 策略/传输、AES-GCM、原子文件写、结构化日志
- `.claude/skills/`：组件开发规范与 Tauri 官方 references
- `docs/explore-develop/`：本次 foundation 迁移探索记录

## 注意

- 当前已通过 Windows MSVC 完整链接和 MSI/NSIS 打包。
- 尚需在真实 Tauri 窗口中手动验证子窗口、托盘、dialog、数据库切换和 subprocess smoke test。
- 发布前请确认 `identifier`、签名、CSP、WebView2 安装策略和实际 subprocess 白名单。

## Recommended IDE Setup

- [VS Code](https://code.visualstudio.com/) + [Tauri](https://marketplace.visualstudio.com/items?itemName=tauri-apps.tauri-vscode) + [rust-analyzer](https://marketplace.visualstudio.com/items?itemName=rust-lang.rust-analyzer)
