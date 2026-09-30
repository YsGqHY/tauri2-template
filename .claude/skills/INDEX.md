# tauri2-template Skills 索引

本项目附带的开发技能库，按**组件**拆分为 5 个 SKILL，覆盖 Rust 后端、前端、IPC、配置权限、构建发布。

调用方式：在对话中直接使用 `/<skill-name>`，或由 AI 根据任务自动匹配加载。

| Skill | 触发场景 | 主要内容 |
|-------|---------|---------|
| [tauri2-rust-backend](./tauri2-rust-backend/SKILL.md) | 写 Rust 侧逻辑、加 command、管理状态、注册插件、拆分模块 | `lib.rs`/`main.rs` 分工、command 定义与注册、async command、`State` 共享状态、错误类型、模块拆分 |
| [tauri2-frontend](./tauri2-frontend/SKILL.md) | 改 UI、写 TS 逻辑、调整 Vite、接入前端框架 | Vanilla TS 无框架模式、DOM 绑定、strict 模式陷阱、Vite 端口约定、接入 React/Vue 的正确步骤 |
| [tauri2-ipc](./tauri2-ipc/SKILL.md) | 前后端通信、invoke 调用、事件收发、参数与错误传递 | `invoke` 与 `#[tauri::command]` 对应关系、camelCase↔snake_case、返回值类型化、`Result` 错误、event emit/listen |
| [tauri2-config-permissions](./tauri2-config-permissions/SKILL.md) | 改窗口配置、加插件、配权限、改 identifier、配 CSP | `tauri.conf.json` 字段、capabilities ACL 模型、加插件三步、CSP 与 `withGlobalTauri` 安全考量 |
| [tauri2-build-release](./tauri2-build-release/SKILL.md) | 构建、打包、优化体积、排查工具链与网络问题 | dev/build 命令、release profile、bundle 目标、Windows 工具链要求、镜像加速、本项目已踩过的坑 |

## 组件与 SKILL 对应表

| 项目路径 | 归属 SKILL |
|---------|-----------|
| `src-tauri/src/lib.rs`、`src-tauri/src/main.rs` | tauri2-rust-backend |
| `src-tauri/Cargo.toml`（依赖与 profile） | tauri2-rust-backend / tauri2-build-release |
| `src-tauri/build.rs` | tauri2-build-release |
| `index.html`、`src/main.ts`、`src/styles.css` | tauri2-frontend |
| `vite.config.ts`、`tsconfig.json`、`package.json` | tauri2-frontend |
| 跨 `src/main.ts` ↔ `src-tauri/src/lib.rs` 的调用链 | tauri2-ipc |
| `src-tauri/tauri.conf.json` | tauri2-config-permissions |
| `src-tauri/capabilities/*.json`、`src-tauri/gen/schemas/` | tauri2-config-permissions |
| `src-tauri/icons/`、构建产物与安装包 | tauri2-build-release |

## 内容来源

每个 SKILL 的入口说明基于**本仓库的真实代码与实测结果**撰写；对应的官方 Tauri v2 原文已同步到各自的 `references/` 目录。共复制 29 份原始 `.md/.mdx` 文件，重复文件按组件独立保存，方便每个 skill 单独查阅。

官方文档同步基准：

- 仓库：`tauri-apps/tauri-docs`
- 分支：`v2`
- commit：`fb135dc6f6894c62ba41a04128e9cb05564a9b7a`
- 同步日期：2026-09-30
- 上游仓库许可证：MIT

各组件 references 索引：

- [Rust 后端 references](./tauri2-rust-backend/references/INDEX.md)
- [前端 references](./tauri2-frontend/references/INDEX.md)
- [IPC references](./tauri2-ipc/references/INDEX.md)
- [配置与权限 references](./tauri2-config-permissions/references/INDEX.md)
- [构建与发布 references](./tauri2-build-release/references/INDEX.md)

需要访问在线官方文档时的入口：

- 官方文档：https://tauri.app/
- command 与 IPC：https://tauri.app/develop/calling-rust/
- 权限模型：https://tauri.app/security/capabilities/
- 配置参考：https://schema.tauri.app/config/2

每个 SKILL 目录结构：

```
tauri2-<name>/
├── SKILL.md          # 触发条件 + 铁律 + 本项目真实代码示例 + 反例
└── references/
    ├── INDEX.md      # 上游来源、commit、许可与文件映射
    └── ...           # Tauri 官方 .md/.mdx 原文
```

## 版本基准

- Tauri 2.12.0 / tauri-build 2.7.0 / tauri-plugin-opener 2.7.0（均为最新稳定版）
- Rust 1.98.1（stable-x86_64-pc-windows-msvc）
- TypeScript 7.0.2 / Vite 8.3.1 / @tauri-apps/api 2.12.0
- 基准日期：2026-09-30
