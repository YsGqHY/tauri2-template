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
  "productName": "tauri2-template",
  "version": "0.1.0",
  "identifier": "com.tauri2template.dev",
  "build": {
    "beforeDevCommand": "pnpm dev",
    "devUrl": "http://localhost:1420",
    "beforeBuildCommand": "pnpm build",
    "frontendDist": "../dist"
  },
  "app": {
    "withGlobalTauri": true,
    "windows": [{ "title": "tauri2-template", "width": 800, "height": 600 }],
    "security": { "csp": null }
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
| `app.security.csp` | 内容安全策略 | 当前 `null` = 关闭，见第 5 节 |
| `app.withGlobalTauri` | 是否注入 `window.__TAURI__` | 当前 `true`，见第 5 节 |
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

本项目当前权限：

```json
{
  "$schema": "../gen/schemas/desktop-schema.json",
  "identifier": "default",
  "description": "Capability for the main window",
  "windows": ["main"],
  "permissions": ["core:default", "opener:default"]
}
```

含义：名为 `main` 的窗口获得 `core:default` 与 `opener:default` 两组权限。

- `core:default` — Tauri 核心能力基础集（含 IPC，**自定义 command 靠它工作**）
- `opener:default` — `tauri-plugin-opener` 的默认权限集

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
    .plugin(tauri_plugin_opener::init())
    .plugin(tauri_plugin_fs::init())        // ← 新增
```

```json
// 3. capabilities/default.json 授权  ← 最易遗漏
{
  "permissions": [
    "core:default",
    "opener:default",
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
    "opener:default",
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

## 5. 两个默认值的安全影响

本模板为开发便利保留了两个宽松默认值，**生产前应重新评估**：

### CSP 为 null

```json
"security": { "csp": null }
```

关闭了内容安全策略。开发期方便（Vite 的 HMR、内联脚本都不受限），但意味着 webview 不限制脚本来源。**接入任何远程内容、CDN 资源、或渲染用户输入的 HTML 之前必须配置 CSP**：

```json
"security": {
  "csp": "default-src 'self'; img-src 'self' asset: http://asset.localhost; style-src 'self' 'unsafe-inline'"
}
```

纯本地应用风险相对低，但一旦有 `<iframe>`、外链、或把远程数据插入 `innerHTML`，缺 CSP 就是 XSS 直通车 —— 而 Tauri 应用里的 XSS 可以调用已授权的所有 command，危害远大于普通网页。

### withGlobalTauri 为 true

```json
"withGlobalTauri": true
```

向 webview 注入全局 `window.__TAURI__`。便利但扩大攻击面：任何注入的脚本都能直接拿到它调 IPC。若前端统一走 ESM import（本项目 [src/main.ts](../../../src/main.ts#L1) 已是如此），**建议改为 `false`**。

改为 `false` 后 `window.__TAURI__` 不再存在，只能 `import { invoke } from "@tauri-apps/api/core"`，类型检查也更好。

---

## 6. 多窗口的权限隔离

capabilities 按窗口 label 生效。新增窗口时：

```json
// capabilities/default.json —— 主窗口保持完整权限
{ "identifier": "default", "windows": ["main"], "permissions": ["core:default", "opener:default", "fs:default"] }
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

- 插件注册的 Rust 侧写法见 `.claude/skills/tauri2-rust-backend/SKILL.md` 第 7 节。
- `devUrl` 与 Vite 端口的双向约定见 `.claude/skills/tauri2-frontend/SKILL.md` 第 4 节。
- 权限被拒的错误只在前端 devtools 可见，排查方法见 `.claude/skills/tauri2-ipc/SKILL.md` 第 7 节。
- `bundle` 相关字段与打包产物见 `.claude/skills/tauri2-build-release/SKILL.md`。

## 官方 references

对应的 Tauri v2 官方原文已同步到 [`references/`](references/)，索引、上游路径、commit 和许可见 [`references/INDEX.md`](references/INDEX.md)。优先查阅 `security/capabilities.mdx`、`security/permissions.mdx`、`security/csp.mdx` 与 `develop/configuration-files.mdx`。
