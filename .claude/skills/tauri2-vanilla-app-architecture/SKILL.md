---
name: tauri2-vanilla-app-architecture
description: tauri2-template 的 Vanilla TypeScript 应用架构指南。凡是用户要新增或重构 AppShell、Router、Store、typed service、页面 mount/cleanup、主题、i18n、异步页面状态或浏览器/Tauri runtime 兼容，即使只说“加一个页面”或“把 UI 拆模块”，也应使用此技能。
---

# Vanilla TS 应用架构

本项目没有 React/Vue，但也不是把所有逻辑塞进 `main.ts` 的静态页面。当前前端使用 Shell、Router、Store、Services、Contracts、Pages、Theme 和 i18n 组成可清理的页面系统。

---

## 1. 真实结构

| 层 | 文件/目录 | 职责 |
|---|---|---|
| bootstrap | [src/main.ts](../../../src/main.ts) | runtime 检测、主窗口/child bootstrap、服务初始化、全局 cleanup |
| shell | [src/components/app-shell.ts](../../../src/components/app-shell.ts)、[src/components/sidebar.ts](../../../src/components/sidebar.ts) | 标题栏、sidebar、router outlet、toast、滚动行为 |
| router | [src/router/index.ts](../../../src/router/index.ts) | key-based route、keepAlive、mount/cleanup |
| state | [src/store/index.ts](../../../src/store/index.ts) | appInfo、settings、preferences、route、runtimeAvailable |
| services | [src/services/](../../../src/services/index.ts) | typed command、event、dialog、window facade |
| contracts | [src/contracts/types.ts](../../../src/contracts/types.ts) | Rust/TS 手工同步的数据模型 |
| pages | [src/pages/](../../../src/pages/home/index.ts) | 页面渲染、交互和页面级异步状态 |
| theme/i18n | [src/theme/index.ts](../../../src/theme/index.ts)、[src/i18n/index.ts](../../../src/i18n/index.ts) | CSS token、系统主题、语言选择和 fallback |
| shared | [src/shared/dom.ts](../../../src/shared/dom.ts)、[src/shared/scroll.ts](../../../src/shared/scroll.ts) | 转义、事件绑定、滚动和格式化 helper |

基础 Vanilla TS、strict、Vite 端口规则仍见 [tauri2-frontend](../tauri2-frontend/SKILL.md)；本 skill 只负责这套应用层组合方式。

---

## 2. bootstrap 与 runtime 分支

`main.ts` 先找到 `#app`，写入 `document.documentElement.dataset.runtime`，再根据 query 判断主窗口还是 child：

- 没有 `child` query：初始化默认 settings/preferences，创建 `Router` 和 `AppShell`，异步读取 Settings、Preferences、AppInfo，订阅 tray action。
- 有 `child` query：只挂载 `mountChildPage`，不初始化主窗口 shell。
- 启动骨架在 bootstrap 后淡出并移除。
- `beforeunload` 统一取消 i18n、tray、theme media listener、shell 和页面 cleanup。

不要让 child 页面意外挂载主窗口 sidebar，也不要把窗口 query 当作可信业务输入；child 类型和 ID 仍由 Rust 校验。

---

## 3. Page mount/cleanup 契约

每个 page 导出：

```ts
export const mountPage = (container: HTMLElement, props: Props): Cleanup => {
  // render + bind listeners + start async work
  return () => {
    // mark disposed + cancel/unlisten + clear timers + remove DOM
  };
};
```

具体规则：

- `Cleanup` 必须幂等或至少不会重复释放出错。
- 页面卸载前先设置 `disposed=true`，异步回调必须检查，避免回写已删除 DOM。
- 保存每个 DOM listener、Tauri event unlisten、ResizeObserver、timer、child handle。
- 页面自己负责页面级资源；shell/router 只负责调用 cleanup，不猜页面内部细节。
- 通过 `innerHTML` 渲染用户输入时先用 [src/shared/dom.ts](../../../src/shared/dom.ts) 的 `escapeHtml`；纯文本更新优先用 `textContent`。

当前 Home、Settings、Subprocess、Child 都遵循这个模式。X-Pro 的 DataGrid/chart 是演示实现，不要从它推导生产级通用组件契约。

---

## 4. Router 与 keep-alive

[Router](../../../src/router/index.ts) 使用固定 `RouteDefinition`：

```ts
interface RouteDefinition {
  id: RouteId;
  labelKey: string;
  slot: "primary" | "footer";
  keepAlive: boolean;
  mount: Mount;
}
```

切换规则：

- `keepAlive=true`：隐藏 host，保留页面状态和 cleanup。
- `keepAlive=false`：调用 cleanup、移除 host、下次重新 mount。
- `refresh()`：先 cleanup 当前页面，再复用 host 重新 mount。
- 每次 navigate 都同步 `appStore.route` 并通知 sidebar/router subscribers。
- route ID 必须是受限 union；不要用任意 URL 字符串替代当前 key-based router。

新增页面时同时决定它是否应保留表单/订阅状态；不能默认全部 keep-alive，否则事件和内存会长期累积。

---

## 5. Store、service 与 contract

Store 是轻量同步状态容器，不代替后端持久化：

- `AppStore.setState` 只做 shallow patch 和 listener 通知。
- settings/preferences 的真实来源是 Rust SQLite；store 只缓存当前 UI 快照。
- service facade 对每个 command 写明确的返回类型，例如 `Promise<AppSettings>`，页面不要散落裸 `invoke`。
- Rust/TS 没有自动 bindings；Rust `#[serde(rename_all = "camelCase")]` 与 [src/contracts/types.ts](../../../src/contracts/types.ts) 必须人工同步。
- command 错误统一由 [src/api/tauri.ts](../../../src/api/tauri.ts) 归一化，再用 `toDisplayError` 映射本地化提示。

页面状态分三类：

1. **持久状态**：SettingsService/PreferencesService 读写后端。
2. **会话状态**：当前 route、busy、selected、expanded、输出列表，留在 page/store。
3. **运行时资源**：event listener、child handle、observer、timer，必须在 cleanup 释放。

不要用 `localStorage` 假装实现跨会话设置；不要把后端错误对象直接拼进 HTML。

---

## 6. 异步竞态与 busy 状态

Vanilla 页面没有框架自动取消 promise，因此显式处理：

- 交互按钮用 busy key/flag 防重复提交。
- 可能连续触发的请求使用 request ID、pending set 或串行化队列。
- `.then/.catch/.finally` 每个分支都检查 `disposed`。
- 乐观更新必须保存 previous state，失败时恢复 theme、palette、settings 或 DOM。
- 监听建立后再拉取 backend snapshot；例如 subprocess 页面先订阅事件再恢复输出。
- child window `open()` 要处理 event 早到、handle 晚到和页面已卸载三种时序。

不要用“组件已经卸载所以 promise 不会回来”作为假设；浏览器和 Tauri IPC 都可能在卸载后完成。

---

## 7. 主题与 i18n

主题由 [src/theme/index.ts](../../../src/theme/index.ts) 管理：

- 固定 `ThemeToken` 列表写入 `--color-*` CSS variables。
- `system` 使用 `matchMedia('(prefers-color-scheme: dark)')`，并保存 change listener cleanup。
- `dark`、`obsidian`、`custom` 有独立 palette；custom 先校验 hex color，再用 base mode 补齐缺失 token。
- 后端也校验 palette 字段、颜色格式和名称长度；前端校验不能取代后端校验。

i18n 由 [src/i18n/index.ts](../../../src/i18n/index.ts) 管理：

- 支持 `zh-CN`、`en-US`、`auto`。
- auto 根据 `navigator.languages` 选择语言。
- key 使用点号路径，当前 locale 缺失时回退 en-US，再缺失才返回 key。
- `setChoice` 触发订阅；shell chrome 和当前 route 需要 refresh。
- 新文案必须同时加入 locale dictionary，不要把用户可见英文硬编码到页面或 tray 逻辑。

---

## 8. 浏览器预览与 Tauri runtime

`isTauriRuntime()` 是功能边界：

- 浏览器可预览 DOM、路由、主题、i18n、演示 grid/chart。
- `invokeCommand`、native dialog、窗口、tray、真实 child/subprocess 在浏览器中应返回明确 runtime error 或 no-op listener。
- 这不是安全绕过；真实功能必须在 Tauri 窗口里验证。
- `withGlobalTauri=false` 时不要读取 `window.__TAURI__` 作为业务 API，统一用 ESM imports 和 facade。

页面遇到 runtime error 时用 `toDisplayError` 生成 message + nextStep，并通过 toast/结果区展示，而不是静默失败。

---

## 9. 测试与验证

前端构建：

```bash
pnpm build
```

手动检查：

- 主窗口 bootstrap 与 child bootstrap 不互相污染。
- route 切换后 keep-alive 页面状态符合预期，非 keep-alive 监听已清理。
- Settings theme/locale 更新后 shell chrome 和当前 page 都刷新。
- 失败的异步请求不会在页面卸载后更新 DOM，也不会留下 disabled/busy 状态。
- 自定义 palette 的非法颜色不会写入 backend。
- 浏览器预览能显示 runtime notice，Tauri 运行能调用真实 command/dialog/window。

---

## 10. 反例

❌ 把所有页面逻辑重新塞回 `src/main.ts`。

❌ page mount 没有返回 cleanup，或 cleanup 不释放 event/observer/timer。

❌ 直接在 HTML template 中插入用户 message、路径或错误 detail。

❌ 只更新 store，不调用后端保存持久设置。

❌ 在 promise 完成回调里不检查 `disposed`。

❌ 通过 `localStorage` 保存主题/语言作为正式数据源。

❌ 为了让浏览器预览“看起来成功”而伪造 Tauri IPC、文件选择或窗口控制结果。

---

## 11. 与其他层协调

- command、事件、错误和 Rust/TS 类型同步见 [tauri2-ipc](../tauri2-ipc/SKILL.md)。
- 基础 Vanilla TS/Vite/strict 规则见 [tauri2-frontend](../tauri2-frontend/SKILL.md)。
- child window、tray、dialog、titlebar 细节见 [tauri2-desktop-integration](../tauri2-desktop-integration/SKILL.md)。
- Settings 的 SQLite 数据源和路径切换见 [tauri2-storage-sqlite](../tauri2-storage-sqlite/SKILL.md)。
