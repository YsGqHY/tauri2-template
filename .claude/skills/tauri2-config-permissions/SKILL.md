---
name: tauri2-config-permissions
description: tauri2-template 的配置与权限指南。说明 tauri.conf.json 各字段含义、Tauri v2 capabilities ACL 权限模型、加插件必须同步授权的三步流程，以及 CSP 与 withGlobalTauri 的安全考量。
---

# 配置与权限（tauri.conf.json + capabilities）

Tauri v2 把"应用配置"和"能力授权"分成两套文件：

- [src-tauri/tauri.conf.json](../../../src-tauri/tauri.conf.json) — 窗口、标识符、构建、打包
- [src-tauri/capabilities/default.json](../../../src-tauri/capabilities/default.json) — 前端可调用哪些能力（ACL）

---

## 1. tauri.conf.json 字段解析

本项目当前配置：

```json
{
  "$schema": "https://schema.tauri.app/config/2",
  "productName": "Foundation Desktop",
  "identifier": "com.foundation.desktop",
  "build": {
    "beforeDevCommand": "pnpm dev",
    "devUrl": "http://localhost:1420",
    "beforeBuildCommand": "pnpm build",
    "frontendDist": "../dist"
  },
  "app": {
    "withGlobalTauri": false,
    "windows": [{
      "title": "Foundation Desktop",
      "width": 1280, "height": 800,
      "minWidth": 960, "minHeight": 600,
      "decorations": false, "resizable": true
    }],
    "security": {
      "csp": "default-src 'self'; script-src 'self'; style-src 'self' 'unsafe-inline'; img-src 'self' data: blob:; connect-src 'self' ipc: http://ipc.localhost; object-src 'none'; frame-src 'none'; base-uri 'self'",
      "devCsp": "default-src 'self'; script-src 'self'; style-src 'self' 'unsafe-inline'; img-src 'self' data: blob:; connect-src 'self' ipc: http://ipc.localhost ws://localhost:1420 ws://127.0.0.1:1420; object-src 'none'; frame-src 'none'; base-uri 'self'"
    }
  },
  "bundle": { "active": true, "targets": "all", "icon": [...] }
}
```

| 字段 | 作用 | 改动注意 |
|------|------|---------|
| `productName` | 安装包名、exe 名、默认窗口标题来源 | 改名时同步 5 处，见根 CLAUDE.md |
| `identifier` | 反向域名唯一标识，决定应用数据目录 | **发布前必须改**，不可以 `.app` 结尾 |
| `build.devUrl` | dev 模式加载地址 | 必须与 `vite.config.ts` 的 `port` 一致 |
| `build.frontendDist` | 生产构建读取的前端产物目录 | 改 Vite `build.outDir` 须同步 |
| `build.beforeDevCommand` | `tauri dev` 前自动执行 | 换包管理器时要改（当前 pnpm） |
| `app.windows[]` | 窗口初始属性 | 运行时改窗口用 Rust API，不改这里 |
| `app.security.csp` | 内容安全策略 | 当前生产 CSP 已启用，开发另有 `devCsp`，见第 5 节 |
| `app.withGlobalTauri` | 是否注入 `window.__TAURI__` | 当前 `false`，前端统一使用 ESM import，见第 5 节 |
| `bundle.targets` | 打包格式 | `"all"` 出全部格式，可收窄提速 |

`$schema` 指向官方 schema，在 VS Code 里能获得字段补全与校验。**不要删**。

窗口常用可选字段（按需添加到 `windows[0]`）：

```json
{
  "title": "tauri2-template",
  "width": 800, "height": 600,
  "minWidth": 600, "minHeight": 400,
  "resizable": true,
  "center": true,
  "decorations": true,
  "transparent": false
}
```

`label` 默认为 `"main"` —— capabilities 里的 `windows: ["main"]` 就是匹配这个 label。新增窗口后要在 capabilities 里加上对应 label，否则新窗口没有任何权限。

---

## 2. 权限模型：v2 与 v1 的根本差异

v1 用 `allowlist` 在 `tauri.conf.json` 里开关 API；**v2 改成 capabilities ACL**，粒度更细且按窗口隔离。

本项目主窗口权限：

```json
{
  "$schema": "../gen/schemas/desktop-schema.json",
  "identifier": "default",
  "windows": ["main"],
  "permissions": [
    "core:default",
    "core:window:allow-close",
    "core:window:allow-minimize",
    "core:window:allow-maximize",
    "core:window:allow-unmaximize",
    "core:window:allow-start-dragging",
    "dialog:default"
  ]
}
```

含义：主窗口获得基础 IPC、窗口控制和 dialog 能力。当前 `tauri-plugin-opener` 已不在主窗口 capability 中使用，不能把旧的 `opener:default` 示例当成实际权限。

动态 child 窗口另有 [src-tauri/capabilities/children.json](../../../src-tauri/capabilities/children.json)：

```json
{
  "identifier": "children",
  "windows": ["child-*"],
  "permissions": [
    "core:event:default",
    "core:window:allow-close",
    "core:window:allow-start-dragging"
  ]
}
```

`core:default` 覆盖自定义 command 的基础 IPC；插件 API 和窗口 API 仍需显式列出。child capability 的通配符和动态 label 需要真实 Tauri 窗口验证。

`$schema` 指向的 `../gen/schemas/desktop-schema.json` 由 `build.rs` 在构建时生成，本项目已存在（`gen/schemas/` 下有 4 个文件）。它使编辑器能补全权限标识符。`gen/schemas` 已被 [src-tauri/.gitignore](../../../src-tauri/.gitignore#L7) 忽略，属生成物。

---

## 3. 加插件的三步（最易出错的地方）

**自定义 command 不需要配权限**（`core:default` 已覆盖）。但**插件 API 必须显式授权**。

以加文件系统插件为例：

```bash
# 1. 加依赖（自动改 Cargo.toml 与 package.json）
pnpm tauri add fs
```

```rust
// 2. lib.rs 注册插件
tauri::Builder::default()
    .plugin(tauri_plugin_fs::init())        // ← 新增
```

```json
// 3. capabilities/default.json 授权  ← 最易遗漏
{
  "permissions": [
    "core:default",
    "fs:default"
  ]
}
```

**漏掉第 3 步的症状**：Rust 编译通过、前端构建通过、应用正常启动，但调用该 API 时抛权限错误，且**只在 webview devtools console 可见**。终端无输出。遇到"某个插件 API 调了没反应"先查这里。

`pnpm tauri add` 有时会自动写入 capabilities，但**不要依赖**，装完手动确认。

---

## 4. 细粒度权限与作用域

`<plugin>:default` 是插件预设的默认集合，通常偏保守。需要更细控制时可列具体权限，或加作用域限制路径：

```json
{
  "permissions": [
    "core:default",
    {
      "identifier": "fs:allow-read-text-file",
      "allow": [{ "path": "$APPDATA/myapp/*" }]
    }
  ]
}
```

常用路径变量：`$APPDATA`、`$APPCONFIG`、`$APPDATA`、`$HOME`、`$DESKTOP`、`$DOCUMENT`、`$RESOURCE`。

**安全准则**：不要为了省事给 `fs:allow-read-file` 配 `{ "path": "**" }` —— 那等于把整个文件系统暴露给前端 JS。始终限定到应用自己的目录。

查询可用权限标识符：

```bash
pnpm tauri permission ls            # 列出所有可用权限
pnpm tauri inspect permissions      # 查看当前生效的权限
```

---

## 5. 当前 CSP 与 global API 安全边界

本项目已经启用生产 CSP，并为 Vite HMR 单独提供 `devCsp`：

```json
"security": {
  "csp": "default-src 'self'; script-src 'self'; style-src 'self' 'unsafe-inline'; img-src 'self' data: blob:; connect-src 'self' ipc: http://ipc.localhost; object-src 'none'; frame-src 'none'; base-uri 'self'",
  "devCsp": "default-src 'self'; script-src 'self'; style-src 'self' 'unsafe-inline'; img-src 'self' data: blob:; connect-src 'self' ipc: http://ipc.localhost ws://localhost:1420 ws://127.0.0.1:1420; object-src 'none'; frame-src 'none'; base-uri 'self'"
}
```

生产 CSP 只允许本地资源、Tauri IPC 和 data/blob 图片；不应随意加入 CDN、任意远程脚本、iframe 或宽泛 `connect-src`。`style-src 'unsafe-inline'` 是当前 Vanilla TS 模板为动态 CSS/主题保留的取舍，接入远程内容前必须重新评估。

`withGlobalTauri` 当前为 `false`：

```json
"withGlobalTauri": false
```

前端统一使用 ESM import（`@tauri-apps/api/core`、`@tauri-apps/api/event`、插件包）和 facade，不依赖 `window.__TAURI__`。这降低全局注入面，也让 TypeScript 能检查调用。

---

## 6. 多窗口的权限隔离

capabilities 按窗口 label 生效。当前主窗口和动态 child 窗口的边界是：

```json
// capabilities/default.json
{
  "identifier": "default",
  "windows": ["main"],
  "permissions": [
    "core:default",
    "core:window:allow-close",
    "core:window:allow-minimize",
    "core:window:allow-maximize",
    "core:window:allow-unmaximize",
    "core:window:allow-start-dragging",
    "dialog:default"
  ]
}
```

```json
// capabilities/children.json
{
  "identifier": "children",
  "windows": ["child-*"],
  "permissions": [
    "core:event:default",
    "core:window:allow-close",
    "core:window:allow-start-dragging"
  ]
}
```

`capabilities/` 下的 JSON 会自动加载，无需在 Rust 再注册。新增动态窗口时必须同时检查真实 label、通配符是否被运行时接受、事件权限是否足够，以及 Rust command 是否仍做调用方授权。capability 不能替代 `child_windows.rs` 对“只能关闭自身/只能向 blank child 发消息”的业务检查。

不要把主窗口的 `dialog:default`、未来的 fs 或 opener 权限复制给 child，除非 child 真的需要。

---

capabilities 按窗口 label 生效。新增窗口时：

```json
// capabilities/default.json —— 主窗口保持完整权限
{ "identifier": "default", "windows": ["main"], "permissions": ["core:default", "fs:default"] }
```

```json
// capabilities/settings.json —— 设置窗口只给最小权限
{ "identifier": "settings", "windows": ["settings"], "permissions": ["core:default"] }
```

`capabilities/` 下所有 json 文件自动加载，无需注册。**这是 v2 相比 v1 的实质优势**：渲染不可信内容的窗口可以被限制到几乎无权限，而 v1 的 allowlist 是全局的。

---

## 7. 反例

❌ 发布时仍用模板的 identifier：

```json
"identifier": "com.tauri2template.dev"   // 必须改成自己的反向域名
```

❌ 改了 Vite 端口但没改 devUrl：

```
vite.config.ts: port: 3000
tauri.conf.json: "devUrl": "http://localhost:1420"   // 窗口白屏
```

❌ 装了插件只做两步，漏授权：

```rust
.plugin(tauri_plugin_fs::init())   // capabilities 里没加 "fs:default" → 运行时被拒
```

❌ 权限作用域开成全盘：

```json
{ "identifier": "fs:allow-read-file", "allow": [{ "path": "**" }] }   // 整个文件系统暴露给前端
```

❌ 在 `tauri.conf.json` 里试图动态改窗口尺寸 —— 该文件是静态配置，运行时调整窗口要用 Rust 侧 API：

```rust
window.set_size(tauri::LogicalSize::new(1024.0, 768.0))?;
```

---

## 8. 与其他层的协调

- 插件注册的 Rust 侧写法见 [tauri2-rust-backend](../tauri2-rust-backend/SKILL.md) 第 7 节。
- `devUrl` 与 Vite 端口的双向约定见 [tauri2-frontend](../tauri2-frontend/SKILL.md) 第 4 节。
- child window、tray、dialog 和窗口控制的实际组合见 [tauri2-desktop-integration](../tauri2-desktop-integration/SKILL.md)。
- 权限被拒的错误只在前端 devtools 可见，排查方法见 [tauri2-ipc](../tauri2-ipc/SKILL.md) 第 7 节。
- `bundle` 相关字段与打包产物见 [tauri2-build-release](../tauri2-build-release/SKILL.md)。

## 官方 references

对应的 Tauri v2 官方原文已同步到 [`references/`](references/)，索引、上游路径、commit 和许可见 [`references/INDEX.md`](references/INDEX.md)。优先查阅 `security/capabilities.mdx`、`security/permissions.mdx`、`security/csp.mdx` 与 `develop/configuration-files.mdx`。
