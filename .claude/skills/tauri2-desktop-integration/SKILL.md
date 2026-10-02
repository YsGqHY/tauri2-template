---
name: tauri2-desktop-integration
description: tauri2-template 的原生桌面集成指南。凡是用户要新增或修改自绘标题栏、窗口控制、动态子窗口、跨窗口消息、系统托盘、原生文件对话框或多窗口 capability，即使只说“加一个弹窗/托盘菜单”，也应使用此技能。
---

# 原生桌面集成

本项目的桌面集成分成两层：Rust 负责窗口/托盘/dialog 的真实能力和权限边界，Vanilla TS 负责 typed service、事件订阅、页面生命周期和浏览器预览降级。

---

## 1. 负责范围与真实文件

| 领域 | 文件 | 职责 |
|---|---|---|
| 主窗口配置 | [src-tauri/tauri.conf.json](../../../src-tauri/tauri.conf.json) | 无边框窗口、尺寸、CSP、devCsp |
| 窗口 command | [src-tauri/src/commands/windows.rs](../../../src-tauri/src/commands/windows.rs) | minimize/maximize/close、child command、调用方校验 |
| 子窗口领域 | [src-tauri/src/child_windows.rs](../../../src-tauri/src/child_windows.rs) | label、类型/尺寸/message 校验、状态、结果和消息事件 |
| 托盘 | [src-tauri/src/tray.rs](../../../src-tauri/src/tray.rs) | menu、show/home/settings/quit action |
| capability | [src-tauri/capabilities/default.json](../../../src-tauri/capabilities/default.json)、[src-tauri/capabilities/children.json](../../../src-tauri/capabilities/children.json) | 主窗口/子窗口 ACL 隔离 |
| 前端服务 | [src/services/child-window.ts](../../../src/services/child-window.ts)、[src/services/native-dialogs.ts](../../../src/services/native-dialogs.ts)、[src/services/window.ts](../../../src/services/window.ts) | typed facade、runtime 检测、监听清理 |
| 前端组件/页面 | [src/components/title-bar.ts](../../../src/components/title-bar.ts)、[src/pages/child/index.ts](../../../src/pages/child/index.ts) | 自绘标题栏和 child UI |

不要在页面里直接构造 `WebviewWindow` 或散落原生 API；页面调用 service，service 再走 command/event contract。

---

## 2. 自绘标题栏与窗口控制

`tauri.conf.json` 的主窗口设置了 `decorations=false`，前端通过 [src/components/title-bar.ts](../../../src/components/title-bar.ts) 渲染拖拽区和三个控制按钮：

- `data-tauri-drag-region` 允许拖动标题栏。
- `data-tauri-no-drag` 避免按钮区域被拖拽吞掉点击。
- minimize/maximize/close 统一交给 `WindowService`。
- Tauri runtime 中优先调用 `getCurrentWindow()`；浏览器预览时才回退到 command，并由 runtime guard 统一报错。
- 所有异步窗口操作都必须 catch，交给 shell 的错误 toast。

窗口操作权限不是自定义 command 的权限，而是 `core:window:allow-*`。增加窗口 API 时同时检查 Rust command、前端 service 和 capability，避免“编译通过但按钮无效”。

---

## 3. 动态 child window 生命周期

当前支持 `confirm`、`message`、`blank` 三类 child。打开流程：

1. 前端生成只含字母、数字、`-`、`_` 的最多 64 字符 ID。
2. `ChildWindowService.open` 先订阅 result/closed/send/broadcast 事件，再调用 `open_child_window`，避免极快窗口在监听建立前结束。
3. Rust 将 label 设为 `child-<id>`，校验 kind、默认/最小/最大尺寸和 message 长度（最多 4096 字符）。
4. 已存在的 label 只 show/focus，不重复创建。
5. 子窗口通过 `index.html?child=...&id=...&message=...` 进入 child bootstrap；query 参数必须 URL encode，不能放敏感信息。
6. `Destroyed` 事件清除 `AppState.child_windows` 并 emit `child:closed:<id>`。
7. child 完成结果时 emit `child:result:<id>`，再关闭窗口；前端 dispose 所有监听。

子窗口 command 的调用方约束：

- 只有主窗口能打开和列出 child。
- child 只能关闭自身；主窗口可以关闭 child。
- child 不能向其他 child 发送点对点消息。
- 点对点消息当前只允许发给 `blank` child；广播使用固定 `child:broadcast` event。

这些是业务授权规则，不要仅依赖 capability 文件代替 Rust 校验。

---

## 4. 多窗口 capability

主窗口 capability 在 [src-tauri/capabilities/default.json](../../../src-tauri/capabilities/default.json)：

- `core:default`
- close/minimize/maximize/unmaximize/start-dragging
- `dialog:default`

子窗口 capability 在 [src-tauri/capabilities/children.json](../../../src-tauri/capabilities/children.json)：

- 匹配 `child-*`
- `core:event:default`
- close/start-dragging

新增 child 需要同时确认：真实 window label、capability 的匹配规则、事件权限和 command 调用方检查。动态 label 与通配符尚未在真实 Tauri 窗口中验证，不要把静态 JSON 检查当成运行时证明。

---

## 5. 托盘与动作事件

`tray::init` 创建 Show/Home/Settings/Quit 菜单。点击前三项时：

1. 显示并 focus 主窗口。
2. emit `tray:action`，payload 是 `{ action }`。
3. 前端 `TrayService.subscribe` 导航到 home/settings，show 则显示 toast。

Quit 直接调用 `app.exit(0)`，应用退出流程由 `AppState::stop_and_join` 收尾。托盘菜单文本当前仍硬编码为中英文，若要接入 i18n，必须设计 Rust/前端共享的稳定 action ID，而不是让前端解析菜单文本。

---

## 6. 原生 dialog 与浏览器预览

[src/services/native-dialogs.ts](../../../src/services/native-dialogs.ts) 封装：

- 单文件/多文件/目录选择
- 保存文件
- confirm
- info/warning/error message

每个方法先执行 `isTauriRuntime()`；浏览器中返回 `DIALOG_RUNTIME_REQUIRED`，页面应显示可理解的下一步，不要静默模拟文件系统写入。Settings 页面用 native confirm 处理数据库覆盖和表清理，取消操作返回 `null`，不能当作失败。

新增 dialog 插件能力时同步三处：`package.json`/Cargo 依赖、`lib.rs` 插件注册、主窗口 capability 权限。完整 ACL 规则见 [tauri2-config-permissions](../tauri2-config-permissions/SKILL.md)。

---

## 7. 事件监听与清理

跨窗口事件必须通过 [src/api/events.ts](../../../src/api/events.ts) 的 `listenEvent`/`safeListen`：

- 非 Tauri runtime 返回 no-op unlisten。
- 监听注册失败时 `safeListen` 不让页面崩溃。
- 页面保存每个 unlisten，并在 cleanup/beforeunload 中调用。
- `ChildWindowService.open` 在订阅、IPC 打开、页面卸载三种竞态下都要释放监听。

事件名称集中在 [src/shared/events.ts](../../../src/shared/events.ts)，不要在多个页面手写 `child:result:${id}`。

---

## 8. 测试与验证

至少验证：

- 主窗口可拖动、最小化、最大化切换、关闭。
- 三种 child kind 能打开、返回结果、被销毁时发 closed。
- 非法 ID、kind、尺寸、超长 message 被拒绝。
- child 不能关闭其他 child 或向其他 child 点对点发消息。
- tray show/home/settings 能恢复主窗口并导航，quit 能触发应用退出清理。
- Settings 文件选择、覆盖确认、取消和浏览器预览错误路径。
- 主窗口和 child capability 在真实 Tauri 窗口中生效。

当前仓库尚未完成真实 child/tray/dialog smoke test；不要把 `pnpm build` 或 `cargo check` 写成这些功能已验证。

---

## 9. 反例

❌ 在主窗口 HTML 中用 `window.open` 代替 Tauri child window。

❌ 只在前端限制 child ID，Rust command 不校验调用方和目标。

❌ 打开 child 后才注册 result listener，导致极快结果丢失。

❌ 把初始 message、token 或密码直接塞进 URL query。

❌ 给 `child-*` capability 复制主窗口全部插件权限。

❌ 在浏览器预览中假装 native dialog 成功并写入本地路径。

❌ 页面卸载时不调用 unlisten，导致旧窗口继续更新已删除 DOM。

---

## 10. 与其他层协调

- IPC 参数、返回值、事件 payload 和错误 shape 见 [tauri2-ipc](../tauri2-ipc/SKILL.md)。
- capability、CSP、插件授权见 [tauri2-config-permissions](../tauri2-config-permissions/SKILL.md)。
- Rust Builder、State、退出清理见 [tauri2-rust-backend](../tauri2-rust-backend/SKILL.md)。
- Vanilla TS 页面 mount/cleanup 和 runtime fallback 见 [tauri2-vanilla-app-architecture](../tauri2-vanilla-app-architecture/SKILL.md)。
