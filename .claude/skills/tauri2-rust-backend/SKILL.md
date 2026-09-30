---
name: tauri2-rust-backend
description: tauri2-template 的 Rust 后端开发指南。说明 lib.rs 与 main.rs 的分工、如何定义与注册 tauri command、async command、用 State 共享状态、错误类型设计，以及业务代码增长后如何拆分模块。
---

# Rust 后端（src-tauri）

本项目 Rust 侧刻意保持极简：[src-tauri/src/lib.rs](../../../src-tauri/src/lib.rs) 承载全部业务与 Builder 装配，[src-tauri/src/main.rs](../../../src-tauri/src/main.rs) 只做入口。

---

## 1. 文件结构与分工

```
src-tauri/
├── Cargo.toml          # 依赖 + [lib] 名 + release profile
├── build.rs            # tauri_build::build()，勿改
└── src/
    ├── lib.rs          # ← 业务代码写这里：command 定义 + Builder 装配
    └── main.rs         # ← 只允许一行 run() 调用，不放业务代码
```

**为什么分成两个文件**：移动端（iOS/Android）不走 `main` 入口，而是由系统调用 `#[cfg_attr(mobile, tauri::mobile_entry_point)]` 标注的 `run()`。业务写在 `lib.rs` 才能同时支持桌面与移动端。

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

`lib.rs` 超过 200 行就该拆。推荐按领域分文件：

```
src/
├── lib.rs              # 只留 run() 与 generate_handler!
├── main.rs
├── commands/
│   ├── mod.rs          # pub use 各子模块
│   ├── config.rs       # 配置相关 command
│   └── file.rs         # 文件相关 command
└── services/
    └── storage.rs      # 纯逻辑，不带 #[tauri::command]
```

```rust
// lib.rs
mod commands;
mod services;

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![
            commands::config::load_config,
            commands::config::save_config,
            commands::file::read_file,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
```

**command 函数保持瘦**：把真实逻辑放 `services/` 里的普通函数，command 只做参数转换与错误映射。这样逻辑可以直接 `cargo test`，而 command 本身难以单测。

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

- 新增 command 后，前端调用方式见 `.claude/skills/tauri2-ipc/SKILL.md`。
- 用到插件能力时，权限配置见 `.claude/skills/tauri2-config-permissions/SKILL.md`。
- `Cargo.toml` 的 `[profile.release]` 已配好体积优化（lto/strip/panic=abort），含义见 `.claude/skills/tauri2-build-release/SKILL.md`。
- `panic = "abort"` 意味着 release 下 panic 不会 unwind，**不能**用 `catch_unwind` 兜错。依赖该行为前先改 profile。

## 官方 references

对应的 Tauri v2 官方原文已同步到 [`references/`](references/)，索引、上游路径、commit 和许可见 [`references/INDEX.md`](references/INDEX.md)。优先查阅 `develop/calling-rust.mdx`、`develop/state-management.mdx` 与 `start/project-structure.mdx`。
