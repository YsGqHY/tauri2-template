---
name: tauri2-frontend
description: tauri2-template 的前端开发指南。说明 Vanilla TS 无框架模式下的 DOM 绑定写法、TypeScript strict 模式的实际陷阱、Vite 与 Tauri 的端口约定，以及后续接入 React/Vue 等框架的正确步骤。
---

# 前端层（Vanilla TS + Vite）

本项目前端仍刻意**不带框架**，但已经不是单一表单壳：使用 `AppShell + Router + Store + Services + Pages` 组织多页面工作台。复杂的页面生命周期、主题、i18n 和 service 组合见 [tauri2-vanilla-app-architecture](../tauri2-vanilla-app-architecture/SKILL.md)，本 skill 保留 Vanilla TS、Vite 和 strict 基础规则。

---

## 1. 文件结构

```
根目录
├── index.html          # Vite 构建入口（不是 src/ 下！）
├── vite.config.ts      # 端口 1420 固定 + 忽略 src-tauri
├── tsconfig.json       # strict + bundler 解析
└── src/
    ├── main.ts         # 主窗口/child bootstrap
    ├── api/             # invoke/event/runtime facade
    ├── components/      # AppShell、Sidebar、TitleBar
    ├── contracts/       # Rust/TS 手工同步类型
    ├── i18n/            # zh-CN/en-US/auto
    ├── pages/           # home/settings/x-pro/subprocess/child
    ├── router/          # key-based Router
    ├── services/        # typed command、dialog、window、tray facade
    ├── store/           # 轻量 app state
    ├── theme/           # CSS token 与系统主题
    ├── shared/          # DOM、事件、滚动 helper
    └── styles.css
```

注意 `index.html` 在**根目录**而非 `src/`，这是 Vite 的约定。`tsconfig.json` 的 `include` 只有 `["src"]`，所以 `vite.config.ts` 本身不被 `tsc` 检查。

---

## 2. DOM 绑定与 mount/cleanup 模式

当前页面不是在 `DOMContentLoaded` 中全局查询一次元素，而是由 Router 挂载到独立 host。页面 mount 时渲染自己的 DOM、绑定事件和启动异步工作，并返回 cleanup：

```ts
export const mountPage = (container: HTMLElement, props: Props): Cleanup => {
  let disposed = false;
  container.innerHTML = `<section class="page">...</section>`;
  const cleanupClick = on(container, "click", handleClick);

  return () => {
    disposed = true;
    cleanupClick();
    container.replaceChildren();
  };
};
```

要点：

- 每个页面自己管理 listener、event unlisten、timer、observer 和异步竞态。
- cleanup 前先设置 `disposed`，promise 回调必须检查它。
- 通过 `qs<T>` 获取元素并判空；不要假定 route host 一定存在。
- 页面只调用 typed service facade，不在模板事件里散落裸 `invoke`。
- 用户输入、路径、错误 detail 进入 `innerHTML` 前必须 `escapeHtml`；纯文本优先 `textContent`。

Router 的 keep-alive、Service/Contract 分层、主题和 i18n 细节见 [tauri2-vanilla-app-architecture](../tauri2-vanilla-app-architecture/SKILL.md)。

---

## 3. TypeScript strict 的实际陷阱

[tsconfig.json](../../../tsconfig.json) 开了这几项，会**直接导致构建失败**而不是警告：

| 选项 | 后果 |
|------|------|
| `strict: true` | `querySelector` 结果不判空即报错 |
| `noUnusedLocals: true` | 声明了没用的变量 → 构建失败 |
| `noUnusedParameters: true` | 函数参数没用到 → 构建失败 |
| `noFallthroughCasesInSwitch: true` | `switch` case 漏 `break` → 构建失败 |

调试时临时注释代码很容易触发 `noUnusedLocals`。参数确实用不到时用下划线前缀绕过：

```ts
window.addEventListener("resize", (_e) => { /* 不用 e */ });
```

`pnpm build` 会跑 `tsc && vite build`，`tsc` 不过则 vite 根本不执行。只想快速看效果可单跑 `pnpm dev`（Vite 不做完整类型检查）。

---

## 4. Vite 与 Tauri 的端口约定

[vite.config.ts](../../../vite.config.ts) 有三处是为 Tauri 定制的，**改动前先理解原因**：

```ts
clearScreen: false,        // 1. 不清屏，否则 Rust 编译错误会被冲掉
server: {
  port: 1420,
  strictPort: true,        // 2. 端口被占直接失败，而非静默换端口
  watch: {
    ignored: ["**/src-tauri/**"],  // 3. 不监听 Rust 目录
  },
}
```

- **`strictPort: true` 必须保留**：Tauri 按 [tauri.conf.json](../../../src-tauri/tauri.conf.json#L8) 的 `devUrl: "http://localhost:1420"` 死等这个端口。静默换端口会导致白窗口。
- **改端口要改两处**：`vite.config.ts` 的 `port` 与 `tauri.conf.json` 的 `devUrl`，漏一处即失联。
- **`watch.ignored` 不能删**：否则 Rust 重编译产生的 `target/` 变动会触发 Vite 无限热更新。

`TAURI_DEV_HOST` 相关的 `host`/`hmr` 分支是移动端真机调试用的，桌面开发时 `host` 为 `false`，无需关心。

---

## 5. 调用后端

页面通过 typed service facade 调用后端；service 再统一经过 [src/api/tauri.ts](../../../src/api/tauri.ts) 的 `invokeCommand<T>`。详细 wire contract 见 [tauri2-ipc](../tauri2-ipc/SKILL.md)：

```ts
// src/services/app.ts
import { invokeCommand } from "../api/tauri";

export const AppService = {
  greet(name: string): Promise<string> {
    return invokeCommand<string>("greet", { name });
  },
};
```

前端要点：

- 给返回值加明确泛型，避免 `unknown` 扩散。
- 可能失败的 command 必须 `try/catch` 或 promise `.catch`，并通过 `toDisplayError` 显示。
- 长期事件监听保存并调用 unlisten；页面 cleanup 不能遗漏。
- `isTauriRuntime()` 为 false 时，IPC facade 返回明确 runtime error，浏览器预览不要伪造成功。

`tauri.conf.json` 当前 `withGlobalTauri: false`，不要依赖 `window.__TAURI__`；统一使用 ESM import 和 facade。

---

## 6. 静态资源

`src/assets/` 下的文件通过 import 引用，Vite 会处理哈希与路径：

```ts
import tauriLogo from "./assets/tauri.svg";
document.querySelector("#logo")?.setAttribute("src", tauriLogo);
```

HTML 里可直接写相对路径（见 [index.html](../../../index.html)）。**不要**用绝对路径 `/src/assets/...`，生产构建后路径结构会变。

需要读取用户文件系统的文件时，那是后端职责 —— 走 command 或 `fs` 插件，不要试图用前端 `fetch("file://...")`，会被 webview 拦。

---

## 7. 接入前端框架的正确步骤

若后续要上 React/Vue，**不要**手动堆依赖，容易漏 tsconfig 与插件配置。推荐重新生成模板再迁移：

```bash
# 在临时目录生成目标框架模板，对比差异后迁移
npm create tauri-app@latest tmp-react -- --template react-ts --tauri-version 2 -y
```

需要手动接入时（以 React 为例），最少改动集合：

```
1. pnpm add react react-dom
2. pnpm add -D @vitejs/plugin-react @types/react @types/react-dom
3. vite.config.ts 加 plugins: [react()]
4. tsconfig.json 加 "jsx": "react-jsx"
5. index.html 的 script src 改指向 .tsx 入口
```

**容易漏第 4 步** —— 漏了会报 "Cannot use JSX unless the '--jsx' flag is provided"。

`src-tauri/` 侧完全不用改：Tauri 只认 `frontendDist` 指向的产物目录，不关心前端用什么框架。

---

## 8. 反例

❌ 硬编码开发地址：

```ts
fetch("http://localhost:1420/api/data");   // 生产构建后此地址不存在
```

❌ 不判空直接用 `querySelector`：

```ts
document.querySelector("#input").value = "x";   // strict 下编译失败
```

❌ 用 `localStorage` 存跨会话状态：

```ts
localStorage.setItem("theme", "dark");   // webview 清缓存即丢失
```

跨会话状态应存到后端（走 command 写文件，或用 `store` 插件）。webview 的存储不可靠。

❌ 删掉 `vite.config.ts` 的 `watch.ignored`：

```ts
server: { port: 1420, strictPort: true }   // 少了 ignored → Rust 重编译触发前端无限刷新
```

❌ 表单提交不阻止默认行为：

```ts
form.addEventListener("submit", () => { greet(); });   // 缺 e.preventDefault() → 白屏
```

---

## 9. 与其他层的协调

- `invoke`、event、错误和 Rust/TS contract 见 [tauri2-ipc](../tauri2-ipc/SKILL.md)。
- Shell、Router、Store、Services、页面 cleanup、主题和 i18n 见 [tauri2-vanilla-app-architecture](../tauri2-vanilla-app-architecture/SKILL.md)。
- child window、tray、dialog、titlebar 见 [tauri2-desktop-integration](../tauri2-desktop-integration/SKILL.md)。
- 端口改动需同步 `tauri.conf.json`，窗口和 capability 配置见 [tauri2-config-permissions](../tauri2-config-permissions/SKILL.md)。
- 前端产物目录 `dist/` 由 `tauri.conf.json` 的 `frontendDist: "../dist"` 指向，改 Vite 的 `build.outDir` 必须同步它。
- `dist/` 已被 [.gitignore](../../../.gitignore#L11) 忽略，不要提交。

## 官方 references

对应的 Tauri v2 官方原文已同步到 [`references/`](references/)，索引、上游路径、commit 和许可见 [`references/INDEX.md`](references/INDEX.md)。优先查阅 `start/frontend/vite.mdx` 与 `start/project-structure.mdx`。
