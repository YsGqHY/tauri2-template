# tauri2-template — Tauri v2 + Vanilla TS 桌面应用脚手架

## 项目愿景

`tauri2-template` 是一个已验证可构建的 Tauri v2 桌面应用起点，为后续 Rust 桌面项目提供：

- 干净的 Rust 后端分层（`lib.rs` 承载业务与 Builder，`main.rs` 仅作入口）
- 已跑通的前后端 IPC 链路（`#[tauri::command]` ↔ `invoke`）
- Tauri v2 的权限模型（capabilities ACL）已就位
- 已实测的 Windows 工具链组合（MSVC 2017 + WebView2 + Rust 1.98.1 可完成链接）
- 国内网络环境下的依赖加速（cargo 走清华 TUNA 镜像）

刻意保持**无前端框架**：前端是 Vanilla TS + Vite，重心放在 Rust 侧。需要 React/Vue 时再自行接入。

## 架构总览

- 后端：Rust 1.98.1（edition 2021），crate `tauri2-template`，lib 名 `tauri2_template_lib`。
- 前端：Vanilla TypeScript 7 + Vite 8，无框架、无路由、无状态库。
- 通信：Tauri v2 IPC — Rust 侧 `#[tauri::command]` 注册到 `invoke_handler`，前端 `invoke("name", args)` 调用。
- 权限：`src-tauri/capabilities/default.json` 显式授权，未列出的能力运行时被拒。
- 构建：`pnpm tauri dev` / `pnpm tauri build`，前端产物 `dist/` 由 `tauri.conf.json` 的 `frontendDist` 指向。

通信链路：

```
index.html (#greet-form / #greet-input / #greet-msg)
    └── src/main.ts: invoke("greet", { name })
        └── [IPC + capabilities ACL 校验]
            └── src-tauri/src/lib.rs: #[tauri::command] fn greet(name: &str)
                └── 注册点：invoke_handler(tauri::generate_handler![greet])
```

## 模块结构图

```mermaid
graph TD
    A["tauri2-template (根)"] --> F["前端层"]
    A --> R["src-tauri/ (Rust 后端)"]
    A --> C["构建配置"]

    F --> FH["index.html<br/>DOM 结构与挂载点"]
    F --> FM["src/main.ts<br/>invoke 调用 + DOM 绑定"]
    F --> FS["src/styles.css"]
    F --> FA["src/assets/"]

    R --> RL["src/lib.rs<br/>command 定义 + Builder 装配"]
    R --> RM["src/main.rs<br/>仅调用 lib::run()"]
    R --> RB["build.rs<br/>tauri_build::build()"]
    R --> RC["tauri.conf.json<br/>窗口/构建/打包配置"]
    R --> RP["capabilities/default.json<br/>权限 ACL"]
    R --> RI["icons/<br/>多平台图标"]

    C --> C1["vite.config.ts<br/>端口 1420 / 忽略 src-tauri"]
    C --> C2["tsconfig.json<br/>strict + bundler 解析"]
```

## 模块索引

| 模块 | 路径 | 语言/技术 | 一句话职责 |
|------|------|-----------|------------|
| 应用入口 | [src-tauri/src/main.rs](src-tauri/src/main.rs) | Rust | 仅调用 `tauri2_template_lib::run()`，不放业务代码 |
| 业务与装配 | [src-tauri/src/lib.rs](src-tauri/src/lib.rs) | Rust | `#[tauri::command]` 定义、插件注册、Builder 装配 |
| 构建脚本 | [src-tauri/build.rs](src-tauri/build.rs) | Rust | `tauri_build::build()`，生成 ACL schema 与资源 |
| 应用配置 | [src-tauri/tauri.conf.json](src-tauri/tauri.conf.json) | JSON | 窗口、标识符、前端产物路径、打包目标 |
| 权限 ACL | [src-tauri/capabilities/default.json](src-tauri/capabilities/default.json) | JSON | 声明 main 窗口可用的权限集合 |
| 前端入口 | [index.html](index.html) | HTML | DOM 结构，Vite 的构建入口 |
| 前端逻辑 | [src/main.ts](src/main.ts) | TypeScript | `invoke` 调用后端、绑定 DOM 事件 |
| 前端构建 | [vite.config.ts](vite.config.ts) | TypeScript | 固定端口 1420、忽略 `src-tauri` 监听 |

## 运行与开发

前置条件：Rust 1.98.1+（MSVC toolchain）、Node.js、pnpm、MSVC 构建工具、WebView2 运行时。

| 命令 | 说明 |
|------|------|
| `pnpm install` | 首次或依赖变更后安装前端依赖 |
| `pnpm tauri dev` | 前后端联调，Rust 改动自动重编译 |
| `pnpm tauri build` | 生产构建并产出安装包 |
| `pnpm build` | 仅前端构建（`tsc && vite build` → `dist/`） |
| `pnpm dev` | 仅 Vite 开发服务器（无 Rust 窗口，仅调 UI 时用） |
| `cargo check` | Rust 快速类型检查（在 `src-tauri/` 下执行，不链接） |
| `cargo build` | Rust 完整构建含链接（在 `src-tauri/` 下执行） |
| `pnpm tauri info` | 输出环境诊断（排查工具链问题第一步） |
| `pnpm tauri add <plugin>` | 添加官方插件（会同时改 Cargo.toml 与 package.json） |

## 改名 / 业务化指南

项目名出现在 5 处，改名需同步，否则编译失败或产物名不一致：

1. [package.json](package.json#L2) 的 `name`
2. [src-tauri/Cargo.toml](src-tauri/Cargo.toml#L2) 的 `[package].name`
3. [src-tauri/Cargo.toml](src-tauri/Cargo.toml#L14) 的 `[lib].name`（须为合法 Rust 标识符，用下划线）
4. [src-tauri/src/main.rs](src-tauri/src/main.rs#L5) 的 crate 引用，须与第 3 步一致
5. [src-tauri/tauri.conf.json](src-tauri/tauri.conf.json#L3) 的 `productName` 与窗口 `title`

另外 `tauri.conf.json` 的 `identifier` 必须改成你自己的反向域名（当前为 `com.tauri2template.dev`）。它决定应用数据目录与系统注册标识，**发布前必须改**，且不要以 `.app` 结尾。

## 权限模型（Tauri v2 与 v1 的关键差异）

v2 用 capabilities ACL 取代了 v1 的 allowlist。[src-tauri/capabilities/default.json](src-tauri/capabilities/default.json) 当前授予 `core:default` 与 `opener:default`。

**加插件必须同时加权限**，否则前端调用在运行时被拒（编译期不报错）：

```
1. pnpm tauri add <plugin>          # 加依赖
2. lib.rs 里 .plugin(<plugin>::init())  # 注册
3. capabilities/default.json 加 "<plugin>:default"  # 授权 ← 最易遗漏
```

详见 `.claude/skills/tauri2-config-permissions/SKILL.md`。

## 测试策略

当前仓库无测试代码。建议：

- Rust：`src-tauri/src/lib.rs` 内 `#[cfg(test)] mod tests`，纯逻辑函数直接 `cargo test`。command 函数体应尽量瘦，把逻辑下沉到可单测的普通函数。
- 前端：无框架，建议 Vitest 覆盖纯函数；涉及 `invoke` 的部分 mock `@tauri-apps/api/core`。
- 端到端：Tauri 官方推荐 WebDriver（tauri-driver）。

## 编码规范

- Rust：`cargo fmt`、`cargo clippy`。command 函数保持瘦，业务逻辑放独立模块。
- 前端：TypeScript `strict: true`，且开了 `noUnusedLocals` / `noUnusedParameters` —— 未使用的变量会**直接构建失败**，不是警告。
- `querySelector` 在 strict 下返回可能为 null，必须判空（见 [src/main.ts](src/main.ts#L7)）。
- 文件路径：仓库内统一正斜杠。

## AI 使用指引

- **分层准则**：业务逻辑与 command 写在 [src-tauri/src/lib.rs](src-tauri/src/lib.rs)（或其拆出的子模块）。[src-tauri/src/main.rs](src-tauri/src/main.rs) 只允许存在 `run()` 调用，**不接受任何业务代码追加**。
- **command 铁律**：新增 `#[tauri::command]` 必须同步注册进 `tauri::generate_handler![]`，漏注册时前端调用报 "command not found"，而 Rust 侧编译通过 —— 这是本模板最易犯的错。
- **权限铁律**：任何插件能力在前端可用前，必须在 `capabilities/default.json` 显式列出。编译期不会提醒你。
- **IPC 参数命名**：前端传 camelCase，Rust 侧收 snake_case，Tauri 自动转换。Rust 侧不要为了迁就前端而写 camelCase 参数名。
- **错误传递**：command 返回 `Result<T, E>` 时 `E` 必须实现 `serde::Serialize`，否则编译失败。裸 `anyhow::Error` 不能直接返回，需转成 `String` 或自定义错误类型。
- **禁止硬编码 devUrl**：前端不要写死 `http://localhost:1420`。端口由 [vite.config.ts](vite.config.ts#L15) 与 [src-tauri/tauri.conf.json](src-tauri/tauri.conf.json#L8) 两处约定，改动须同步。
- **不要让 Vite 监听 src-tauri**：[vite.config.ts](vite.config.ts#L27) 已忽略，移除会导致 Rust 重编译触发前端无限刷新。
- **CSP 当前为 null**：[src-tauri/tauri.conf.json](src-tauri/tauri.conf.json#L22) 关闭了内容安全策略，方便开发但生产不安全。接入远程内容或第三方脚本前必须配置 CSP。
- **withGlobalTauri 为 true**：`window.__TAURI__` 可用。它扩大了前端攻击面，若不需要全局注入建议关掉，改用 ESM import。
- **依赖镜像**：cargo 走清华 TUNA 镜像（配置在用户级 `~/.cargo/config.toml`，不在仓库内）。换机器需自行配置，否则首次构建可能极慢。

## 已验证事实（勿凭猜测推翻）

- Tauri 2.12.0 / tauri-build 2.7.0 / tauri-plugin-opener 2.7.0 均为当前最新稳定版。
- MSVC Build Tools 2017（14.16）+ Windows SDK 10.0.19041 可完成链接，**不必**升级 VS 2022。
- `cargo check` 不做链接，验证工具链完整性必须跑 `cargo build`。
- `generic-array` / `toml` / `toml_datetime` / `toml_edit` 落后于最新版，是 Tauri 上游（`system-deps`、`sha2`）的约束，非本项目问题，不要强行 `--precise` 绕过。
- `pnpm-workspace.yaml` 是 pnpm 发布时间门禁自动生成的例外记录，删除可能导致 plugin-opener 回退到 2.6.0。

## 变更记录 (Changelog)

- 2026-09-30：初始化项目。生成 Tauri v2 vanilla-ts 模板；安装 Rust 1.98.1 MSVC 工具链；统一命名为 `tauri2-template`；配置 cargo 清华镜像；验证 `cargo build` 可链接（13MB exe）。
- 2026-09-30（依赖更新）：TypeScript 6.0.3 → 7.0.2（跨大版本，经隔离实测通过）；`@tauri-apps/plugin-opener` 2.6.0 → 2.7.0，与 Rust 侧版本对齐。Tauri 核心确认已是最新稳定版，未升级至 3.0.0-alpha 预发布版。
- 2026-09-30（文档）：初始化 CLAUDE.md 与 `.claude/skills/` 开发技能库。
- 2026-09-30（官方 references）：从 `tauri-apps/tauri-docs` 的 `v2` 分支（commit `fb135dc6f6894c62ba41a04128e9cb05564a9b7a`）同步 29 份 Tauri 官方 `.md/.mdx` 原文到五个 skill 的 `references/` 目录，并保留 MIT 来源说明。
