---
name: tauri2-rust-backend
description: tauri2-template 的 Rust 后端开发指南。说明 lib.rs 与 main.rs 的分工、如何定义与注册 tauri command、async command、用 State 共享状态、错误类型设计，以及业务代码增长后如何拆分模块。
---

# Rust 后端（src-tauri）

本项目 Rust 侧采用按领域拆分的模块化结构：[src-tauri/src/lib.rs](../../../src-tauri/src/lib.rs) 只负责 Builder/setup、State、插件、command 注册和退出清理；业务逻辑位于 `commands/`、`storage/`、`subprocess.rs` 与 `utils/` 等模块；[src-tauri/src/main.rs](../../../src-tauri/src/main.rs) 仍只做入口调用。

---

## 1. 文件结构与分工

```
src-tauri/
├── Cargo.toml             # 依赖 + [lib] 名 + release profile
├── build.rs               # tauri_build::build()，勿改
├── capabilities/          # 主窗口与 child-* 窗口 ACL
└── src/
    ├── lib.rs             # Builder、setup、State、command 注册、Exit 清理
    ├── main.rs            # 仅调用 lib::run()
    ├── commands/          # 薄 command facade，做窗口/参数/错误边界
    ├── models/            # serde 输入、输出和 event payload
    ├── state.rs            # AppState、锁、取消信号、worker registry
    ├── events.rs           # Tauri event 名称与 payload 发射
    ├── storage/            # SQLite schema、迁移、设置和路径回滚
    ├── subprocess.rs       # 白名单子进程、reader、停止和历史
    ├── process_group.rs    # Unix 进程组与 Windows JobObject/fallback
    ├── child_windows.rs    # 动态子窗口与跨窗口消息
    ├── tray.rs             # 系统托盘菜单与 action event
    └── utils/              # cryptox/filex/httpx/logx 内部工具
```

**为什么分成 lib 与 main**：移动端不走 `main` 入口，而是由系统调用带 `mobile_entry_point` 的 `run()`。领域模块留在 `lib.rs` 所在 crate，才能保持桌面与移动入口一致；`main.rs` 不得追加业务代码。

`main.rs` 当前内容（不要追加业务代码）：

```rust
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    tauri2_template_lib::run()
}
```

> 第一行 `windows_subsystem = "windows"` 让 release 构建不弹出额外的控制台窗口，**不要删**。debug 构建仍保留控制台，方便看 `println!`。

**crate 名为什么带 `_lib` 后缀**：[src-tauri/Cargo.toml](../../../src-tauri/Cargo.toml#L14) 里 `[lib].name = "tauri2_template_lib"`。Windows 上 lib 名与 bin 名相同会冲突（rust-lang/cargo#8519），因此加后缀区分。改名时 `main.rs` 的引用必须同步。

---

## 2. 定义 command

command 是前端唯一能调用 Rust 的入口。本项目现有示例：

```rust
// src-tauri/src/lib.rs
#[tauri::command]
fn greet(name: &str) -> String {
    format!("Hello, {}! You've been greeted from Rust!", name)
}
```

**铁律：加了 `#[tauri::command]` 还必须注册进 `generate_handler!`**，两步缺一不可：

```rust
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![greet])  // ← 新 command 加这里
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
```

漏注册时 **Rust 侧编译完全通过**，只在前端运行时报 `Command greet not found`。这是本模板最容易犯的错误，没有任何编译期保护。

多个 command 用逗号分隔：

```rust
.invoke_handler(tauri::generate_handler![greet, read_config, save_config])
```

---

## 3. 参数与返回值

参数和返回值必须能被 serde 序列化。基础类型、`String`、`Vec<T>`、`Option<T>`、以及 `#[derive(Serialize, Deserialize)]` 的结构体都可以。

```rust
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
struct AppConfig {
    theme: String,
    font_size: u32,
}

#[tauri::command]
fn save_config(config: AppConfig) -> String {
    format!("saved theme={}", config.theme)
}
```

项目已含 `serde` 与 `serde_json` 依赖（[src-tauri/Cargo.toml](../../../src-tauri/Cargo.toml#L23)），直接用。

参数命名见 `.claude/skills/tauri2-ipc/SKILL.md`：**Rust 侧写 snake_case，前端传 camelCase**，Tauri 自动转换。不要为迁就前端在 Rust 里写 camelCase。

---

## 4. 错误处理

command 返回 `Result<T, E>` 时，**`E` 必须实现 `serde::Serialize`**。这是编译期强制的。

❌ 直接返回 `std::io::Error` 或 `anyhow::Error` —— 它们没实现 `Serialize`，编译失败：

```rust
#[tauri::command]
fn read_file(path: String) -> Result<String, std::io::Error> {  // 编译错误
    std::fs::read_to_string(path)
}
```

✅ 最简做法：转成 `String`：

```rust
#[tauri::command]
fn read_file(path: String) -> Result<String, String> {
    std::fs::read_to_string(path).map_err(|e| e.to_string())
}
```

✅ 规范做法：自定义错误类型（业务变复杂后推荐）：

```rust
use serde::Serialize;

#[derive(Debug, Serialize)]
#[serde(tag = "kind", content = "message")]
enum AppError {
    NotFound(String),
    PermissionDenied(String),
    Io(String),
}

impl From<std::io::Error> for AppError {
    fn from(e: std::io::Error) -> Self {
        AppError::Io(e.to_string())
    }
}

#[tauri::command]
fn read_file(path: String) -> Result<String, AppError> {
    Ok(std::fs::read_to_string(path)?)
}
```

`Err` 会让前端的 `invoke` promise **reject**，前端需 `try/catch`。

---

## 5. async command

涉及 IO 或耗时操作时用 `async`，避免阻塞主线程：

```rust
#[tauri::command]
async fn fetch_data(url: String) -> Result<String, String> {
    // 不要在这里用阻塞 IO（std::fs / std::thread::sleep）
    tokio::time::sleep(std::time::Duration::from_secs(1)).await;
    Ok(format!("fetched {}", url))
}
```

**注意**：`async` command 里不要调用阻塞 API，会卡住 async runtime。阻塞任务应包在 `tauri::async_runtime::spawn_blocking` 里，或干脆写成同步 command（Tauri 会在独立线程池执行同步 command）。

Tauri 已通过依赖树引入 tokio；若要直接用 tokio API，在 `Cargo.toml` 显式加 `tokio` 依赖，不要依赖传递依赖。

### 5.1 阻塞工作、线程所有权与取消

- async command 不直接调用 `std::fs`、`std::thread::sleep`、`reqwest::blocking`、长时间同步 SQLite 或其他可能长时间阻塞的 API。有限阻塞任务用 `tauri::async_runtime::spawn_blocking`；短小且有界的逻辑可用同步 command；长生命周期循环、子进程 pipe 读取/监视用专用 `std::thread`，并保存和回收 `JoinHandle`。
- Rayon 只用于经测量确认的 CPU 密集、可分块工作；它不替代 I/O worker，也不是 Go `GOMAXPROCS` 的机械对应。本项目当前没有 Rayon 依赖。
- Rust 线程不会像 Go `context.Context` 一样自动继承取消。显式传递 `CancellationToken` / 原子停止标志，并定义资源清理、超时和退出协议；不可丢弃 join handle 后假设任务已停止。
- **锁不得跨 `.await`**：在 await 前克隆/取出所需数据并释放 `std::sync` guard。只有确实需要跨 await 的短临界区才考虑 async mutex；不得在持锁时做文件、网络、子进程或数据库 I/O。
- 停止/退出必须有界：先协作取消，经过明确宽限期后强制终止，再在 deadline 内 join；外部子进程还需终止整个进程组/树。不可让应用退出无期限等待 worker。

本项目当前子进程实现由 reader 线程加独占 `Child` 的 worker 管理，使用 `AtomicBool` 停止标志；输出快照每流最多 256 行，单行最多 1 MiB，超限追加截断标记，历史最多 64 项。[subprocess.rs](../../../src-tauri/src/subprocess.rs#L92-L190) worker 对子进程有 5 秒优雅期、强杀后 2 秒等待和直接 `Child::kill` 兜底；force termination 后会 reap child。reader shutdown 最多等待 2 秒，超时发出最后手段 detach warning；shutdown lifecycle gate 拒绝迟到的 subprocess 注册，`ready` event 先于 worker 发出，已完成 `JoinHandle` 会被清理。[subprocess.rs](../../../src-tauri/src/subprocess.rs#L190-L237) [process_group.rs](../../../src-tauri/src/process_group.rs#L105-L162) [process_group.rs](../../../src-tauri/src/process_group.rs#L164-L233) Windows JobObject attach 仍是 post-spawn race，失败时退化到 taskkill fallback；Unix runtime 尚未验证。[state.rs](../../../src-tauri/src/state.rs#L82-L108) Foundation 的 external context、`KillGracePeriod`、stdin、env、`CaptureOutput` 通用 API 未暴露为当前 Tauri contract，不能假定 IPC 已等价。

本项目 HTTP 工具使用 `reqwest::blocking`；其 30 秒请求超时限制正在执行的 socket I/O，`CancellationToken` 只在请求前和 retry backoff 切片之间检查，不能立即中断已开始的阻塞请求。[httpx.rs](../../../src-tauri/src/utils/httpx.rs#L261-L300) 从 async command 使用时须放入 `spawn_blocking` 或专用 worker，并向调用方说明最坏取消延迟。

---

## 6. 共享状态（State）

跨 command 共享数据用 `.manage()` + `State<T>`。需要可变时自己加锁：

```rust
use std::sync::Mutex;
use tauri::State;

struct Counter(Mutex<u32>);

#[tauri::command]
fn increment(counter: State<Counter>) -> u32 {
    let mut n = counter.0.lock().unwrap();
    *n += 1;
    *n
}

pub fn run() {
    tauri::Builder::default()
        .manage(Counter(Mutex::new(0)))      // ← 注册状态
        .invoke_handler(tauri::generate_handler![increment])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
```

`State<T>` 参数**不会**出现在前端调用签名里，前端仍只传 `{}`。忘记 `.manage()` 就用 `State<T>` 会在运行时 panic。

async command 里不要持有 `std::sync::MutexGuard` 跨 `await`（不是 `Send`）。这种场景用 `tokio::sync::Mutex`。

---

## 7. 注册插件

```rust
tauri::Builder::default()
    .plugin(tauri_plugin_opener::init())   // 已有
    .plugin(tauri_plugin_fs::init())       // 新增插件
```

**加插件是三步，不是一步**：

```
1. pnpm tauri add fs              # 自动改 Cargo.toml + package.json
2. lib.rs 里 .plugin(tauri_plugin_fs::init())
3. capabilities/default.json 加 "fs:default"   ← 最易遗漏，漏了运行时被拒
```

第 3 步详见 `.claude/skills/tauri2-config-permissions/SKILL.md`。

---

## 8. 业务变大后拆分模块

业务变大后按领域拆分，不要继续堆进 `lib.rs`。当前项目的分层是：

```
src/
├── lib.rs             # 模块声明、Builder/setup、State、注册和退出清理
├── main.rs
├── commands/          # app/settings/storage/windows/subprocess command facade
├── models/            # serde contract
├── storage/           # SQLite 领域逻辑与测试
├── subprocess.rs      # 长生命周期进程 worker
├── process_group.rs   # 平台进程树终止
├── state.rs           # 共享状态与生命周期 gate
├── events.rs          # event 发射 helper
└── utils/             # 内部工具，不直接暴露 IPC
```

```rust
// lib.rs：只做装配，不把 storage/subprocess 算法写进来
let app = tauri::Builder::default()
    .plugin(tauri_plugin_dialog::init())
    .plugin(tauri_plugin_opener::init())
    .setup(|app| {
        let storage = storage::initialize(app.path().app_data_dir()?)?;
        app.manage(AppState::new(storage));
        tray::init(app.handle())?;
        events::start_time_loop(app.handle())?;
        Ok(())
    })
    .invoke_handler(tauri::generate_handler![
        commands::settings::get_app_settings,
        commands::storage::get_storage_stats,
        commands::subprocess::run_subprocess,
    ])
    .build(tauri::generate_context!())?;
```

**command 函数保持瘦**：只做调用方/参数边界和错误映射；真实逻辑放 `storage/`、`subprocess.rs`、`child_windows.rs` 或纯 helper 中。长生命周期 worker 必须由 `AppState` 注册和回收，不能在 command 内 spawn 后遗弃。这样纯逻辑可以直接 `cargo test`，而 command 本身只需做少量集成验证。

---

## 9. 测试

```rust
// services/storage.rs
pub fn normalize_theme(input: &str) -> String {
    match input.trim().to_lowercase().as_str() {
        "dark" | "light" => input.trim().to_lowercase(),
        _ => "light".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn falls_back_to_light() {
        assert_eq!(normalize_theme("DARK"), "dark");
        assert_eq!(normalize_theme("neon"), "light");
    }
}
```

在 `src-tauri/` 下执行 `cargo test`。注意需要 `AppHandle` 或 `State` 的 command 无法直接单测 —— 这正是逻辑要下沉到普通函数的原因。

---

## 10. 反例

❌ 在 `main.rs` 里写业务：

```rust
fn main() {
    let config = load_config();      // 不属于这里
    tauri2_template_lib::run()
}
```

❌ 定义了 command 但忘记注册（编译通过，运行时才炸）：

```rust
#[tauri::command]
fn new_cmd() -> String { "hi".into() }

// invoke_handler 里没加 new_cmd
```

❌ 返回未实现 `Serialize` 的错误类型：

```rust
fn read() -> Result<String, anyhow::Error>   // 编译失败
```

❌ async command 里做阻塞 IO：

```rust
#[tauri::command]
async fn bad() -> String {
    std::thread::sleep(std::time::Duration::from_secs(5));  // 卡住 runtime
    "done".into()
}
```

❌ 删掉 `main.rs` 的 `windows_subsystem` 属性 —— release 构建会多弹一个黑色控制台窗口。

---

## 11. 与其他层的协调

- 新增 command、event、错误和 Rust/TS 类型同步见 [tauri2-ipc](../tauri2-ipc/SKILL.md)。
- SQLite schema、路径切换和设置持久化见 [tauri2-storage-sqlite](../tauri2-storage-sqlite/SKILL.md)。
- 子进程、取消、进程组和退出清理见 [tauri2-subprocess-lifecycle](../tauri2-subprocess-lifecycle/SKILL.md)。
- child window、tray、dialog 和窗口 ACL 见 [tauri2-desktop-integration](../tauri2-desktop-integration/SKILL.md)。
- cryptox/filex/httpx/logx 见 [tauri2-rust-utils](../tauri2-rust-utils/SKILL.md)。
- 用到插件能力时，权限配置见 [tauri2-config-permissions](../tauri2-config-permissions/SKILL.md)。
- `Cargo.toml` 的 `[profile.release]` 已配好体积优化（lto/strip/panic=abort），含义见 [tauri2-build-release](../tauri2-build-release/SKILL.md)。
- `panic = "abort"` 意味着 release 下 panic 不会 unwind，**不能**用 `catch_unwind` 兜错。依赖该行为前先改 profile。

## 官方 references

对应的 Tauri v2 官方原文已同步到 [`references/`](references/)，索引、上游路径、commit 和许可见 [`references/INDEX.md`](references/INDEX.md)。优先查阅 `develop/calling-rust.mdx`、`develop/state-management.mdx` 与 `start/project-structure.mdx`。
