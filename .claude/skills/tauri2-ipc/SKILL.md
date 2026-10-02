---
name: tauri2-ipc
description: tauri2-template 的前后端通信指南。说明 invoke 与 #[tauri::command] 的对应规则、camelCase 与 snake_case 自动转换、返回值类型化、Result 错误如何变成 promise reject，以及事件（emit/listen）双向通信。
---

# IPC 通信（前端 ↔ Rust）

本项目前后端唯一通信方式是 Tauri v2 IPC。两条路径：**command**（前端主动调用，请求-响应）与 **event**（Rust/前端之间的广播，无返回值）。页面不直接散落裸 `invoke`，而是通过 typed service facade 和统一事件 helper。

当前链路：

```
src/pages/* 或 src/components/*
    └── src/services/*.ts: invokeCommand<T>(name, args)
        └── src/api/tauri.ts: runtime 检测 + AppError 规范化
            └── commands/*.rs: #[tauri::command]
                └── lib.rs: generate_handler![commands::...]

Rust worker / tray / child window
    └── src-tauri/src/events.rs: app.emit(event, payload)
        └── src/api/events.ts: listenEvent / safeListen
            └── 页面保存 unlisten，并在 cleanup 时释放
```

---

## 1. command 的三处必须一致

新增一个 command 要同时动三个地方，漏任何一处都失败：

| # | 位置 | 内容 | 漏掉的后果 |
|---|------|------|-----------|
| 1 | `commands/*.rs` | `#[tauri::command] pub fn my_cmd(...)` | 前端报 command not found |
| 2 | `lib.rs` | `generate_handler![commands::...::my_cmd]` | **Rust 编译通过**，前端运行时报 not found |
| 3 | `services/*.ts` | `invokeCommand<T>("my_cmd", {...})` | 页面没有可用 facade |

**第 2 步没有任何编译期保护**，是本项目最高频的错误来源。写完 command 立刻检查 [src-tauri/src/lib.rs](../../../src-tauri/src/lib.rs) 的注册列表。

调用名是 **Rust 函数名的字符串形式**，不是文件名、不是模块路径：

```rust
// src-tauri/src/commands/app.rs
#[tauri::command]
pub fn get_app_info(app: AppHandle) -> AppResult<AppInfo> { /* ... */ }
```

```ts
// src/services/app.ts
await invokeCommand<AppInfo>("get_app_info"); // ✅
await invokeCommand<AppInfo>("getAppInfo");  // ❌ 找不到
```

模块拆分后 `generate_handler!` 里写完整路径，但前端调用名仍是**函数名**：

```rust
.invoke_handler(tauri::generate_handler![
    commands::app::get_app_info,
    commands::storage::get_storage_stats,
])
```

```ts
await invokeCommand<StorageStats>("get_storage_stats"); // 不带模块前缀
```

```ts
// main.ts —— 名字必须逐字符一致
await invoke("load_config");   // ✅
await invoke("loadConfig");    // ❌ 找不到
```

模块拆分后 `generate_handler!` 里写路径，但前端调用名仍是**函数名**：

```rust
.invoke_handler(tauri::generate_handler![commands::config::load_config])
```

```ts
await invoke("load_config");   // 不带模块前缀
```

---

## 2. 参数命名：camelCase ↔ snake_case

Tauri 自动转换。**Rust 侧写地道的 snake_case，前端写地道的 camelCase**：

```rust
#[tauri::command]
fn save_file(file_path: String, is_readonly: bool) -> Result<(), String> {
    Ok(())
}
```

```ts
await invoke("save_file", { filePath: "a.txt", isReadonly: true });
```

不要为了"对齐"而在 Rust 里写 camelCase 参数名。若确实需要固定名称，用 serde 显式重命名：

```rust
#[tauri::command]
fn save_file(#[serde(rename = "path")] file_path: String) {}
```

参数是**对象形式**传递，不是位置参数：

```ts
await invoke("greet", { name: "world" });   // ✅ 对象
await invoke("greet", "world");             // ❌ 不支持
```

无参数时可省略第二个实参：`await invoke("get_version")`。

---

## 3. 返回值类型化

`invoke` 默认返回 `Promise<unknown>`，务必用泛型标注，否则 `unknown` 会污染下游：

```ts
const msg = await invoke<string>("greet", { name: "x" });
const count = await invoke<number>("get_count");
const cfg = await invoke<{ theme: string; fontSize: number }>("load_config");
```

Rust 结构体到 TS 类型的映射需**手工保持同步**（Tauri 不像 Wails 会生成 bindings）。字段名遵循 serde 的序列化结果 —— 默认是 Rust 的 snake_case：

```rust
#[derive(Serialize)]
struct Config {
    font_size: u32,      // → JSON 里是 font_size
}
```

若想让前端拿到 camelCase，在 Rust 侧显式声明：

```rust
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Config {
    font_size: u32,      // → JSON 里是 fontSize
}
```

**注意这与参数方向不对称**：参数（前端→Rust）自动转换，返回值（Rust→前端）**不会**自动转 camelCase，由 serde 决定。建议统一加 `#[serde(rename_all = "camelCase")]` 避免混乱。

Rust 返回 `()` 时前端得到 `null`。

---

## 4. 错误：Result 变成 promise reject

command 返回 `Result<T, E>` 时，`Ok` → resolve，`Err` → **reject**。前端必须 `try/catch`：

```rust
#[tauri::command]
fn read_file(path: String) -> Result<String, String> {
    std::fs::read_to_string(&path).map_err(|e| e.to_string())
}
```

```ts
try {
  const content = await invoke<string>("read_file", { path: "a.txt" });
} catch (err) {
  console.error("读取失败:", err);   // err 就是 Rust 的 Err 值
}
```

未捕获的 reject 只在 devtools 里显示 unhandled rejection，UI 上表现为"点了没反应"，排查困难。**凡是返回 `Result` 的 command，前端一律包 try/catch**。

`E` 必须实现 `serde::Serialize`（编译期强制），详见 `.claude/skills/tauri2-rust-backend/SKILL.md` 第 4 节。

---

## 5. 事件（双向广播）

command 是前端发起的请求-响应。需要 **Rust 主动推送**给前端时用事件。

Rust 侧发射（需要 `AppHandle` 或 `Window`）：

```rust
use tauri::{AppHandle, Emitter};

#[tauri::command]
fn start_task(app: AppHandle) {
    std::thread::spawn(move || {
        for i in 1..=5 {
            app.emit("progress", i).unwrap();
            std::thread::sleep(std::time::Duration::from_secs(1));
        }
    });
}
```

前端监听：

```ts
import { listen } from "@tauri-apps/api/event";

const unlisten = await listen<number>("progress", (event) => {
  console.log("进度:", event.payload);
});

// 不再需要时必须取消监听，否则内存泄漏
unlisten();
```

本项目的实际封装：

```rust
// src-tauri/src/events.rs
pub fn emit_subprocess_exit(app: &AppHandle, payload: SubprocessExitPayload) -> AppResult<()> {
    app.emit(&format!("subprocess:exit:{}", payload.id), payload)?;
    Ok(())
}
```

```ts
// src/api/events.ts
const unlisten = await listenEvent<ProcessExit>(eventName, handler);
// page cleanup 时调用 unlisten()
```

子窗口、subprocess、tray、time event 的名称集中在 [src/shared/events.ts](../../../src/shared/events.ts)；动态事件名必须由同一个 helper 生成。`safeListen` 在浏览器 runtime 或监听失败时返回 no-op cleanup，页面仍要保存它。

要点：

- `AppHandle` 作为 command 参数时**不占用前端调用签名**，前端仍只传业务参数。
- `emit` 广播到所有窗口；事件 payload 字段由 Rust `serde` 决定。
- 事件名没有编译期校验，集中管理并在两侧维护同名契约。
- **长期监听必须保存并调用 `unlisten`**，尤其在 page/router cleanup 时。
- 子进程等高频事件只传有界行/状态；完整快照通过 command 拉取，不要把无限日志塞进 event。


---

## 6. 权限的影响

**自定义 command 不需要配权限**，`capabilities/default.json` 里的 `core:default` 已覆盖 IPC 基础能力。

但**插件提供的 API 需要显式授权**。例如装了 `fs` 插件后直接在前端 `import { readTextFile } from "@tauri-apps/plugin-fs"` 调用，若没在 capabilities 里加 `fs:default`，运行时抛权限错误 —— 编译与构建阶段都不报错。详见 `.claude/skills/tauri2-config-permissions/SKILL.md`。

---

## 7. 调试手段

- Rust 侧 `println!` / `dbg!` 输出在跑 `pnpm tauri dev` 的终端（debug 构建保留控制台）。
- 前端 `console.log` 在 webview devtools（窗口内右键 → 检查，或 F12）。
- command 未找到、权限被拒这类错误只在**前端 devtools console** 可见，终端不显示，排查时先开 devtools。
- 怀疑参数序列化问题时，Rust 侧把参数收成 `serde_json::Value` 打印原始结构。

---

## 8. 反例

❌ 新增 command 忘记注册（最高频错误）：

```rust
#[tauri::command]
fn my_cmd() -> String { "x".into() }

.invoke_handler(tauri::generate_handler![greet])   // 漏了 my_cmd，编译通过但前端调不到
```

❌ 用位置参数调用：

```ts
await invoke("greet", "world");        // ❌ 必须是 { name: "world" }
```

❌ 调用名写成 camelCase：

```ts
await invoke("loadConfig");            // ❌ Rust 函数叫 load_config
```

❌ 返回值不标类型，任由 `unknown` 扩散：

```ts
const msg = await invoke("greet", { name: "x" });
msg.toUpperCase();                     // ❌ unknown 上没有此方法
```

❌ 返回 `Result` 的 command 不捕获异常：

```ts
const c = await invoke<string>("read_file", { path: "x" });   // 文件不存在 → 静默 unhandled rejection
```

❌ 监听事件不取消：

```ts
await listen("progress", handler);     // 没保存 unlisten → 泄漏
```

---

## 9. 与其他层的协调

- command 的定义、错误类型、async 写法见 [tauri2-rust-backend](../tauri2-rust-backend/SKILL.md)。
- 前端 Shell、页面 cleanup、service facade 见 [tauri2-vanilla-app-architecture](../tauri2-vanilla-app-architecture/SKILL.md)。
- 基础 DOM/Vite/strict 规则见 [tauri2-frontend](../tauri2-frontend/SKILL.md)。
- 插件 API、CSP 和 capability 授权见 [tauri2-config-permissions](../tauri2-config-permissions/SKILL.md)。
- child window/tray/dialog 的具体事件语义见 [tauri2-desktop-integration](../tauri2-desktop-integration/SKILL.md)。
- subprocess 的 ready/stdout/stderr/exit 顺序和快照见 [tauri2-subprocess-lifecycle](../tauri2-subprocess-lifecycle/SKILL.md)。
- 本项目**没有**自动生成 bindings（不同于 Wails），Rust 类型与 TS 类型需人工同步；集中维护在 [src/contracts/types.ts](../../../src/contracts/types.ts)，不要让页面散落内联结构体逐渐漂移。

## 官方 references

对应的 Tauri v2 官方原文已同步到 [`references/`](references/)，索引、上游路径、commit 和许可见 [`references/INDEX.md`](references/INDEX.md)。优先查阅 `develop/calling-rust.mdx`、`develop/calling-frontend.mdx` 与 `concept/Inter-Process Communication/index.mdx`。
