# tauri2-vanilla-app-architecture references

本 skill 是基于本仓库 Vanilla TS 应用壳提炼的领域指南，不复制新的上游文档。页面 mount/cleanup、路由和 service facade 是行为真相。

## 本仓库实现

- bootstrap：[src/main.ts](../../../../src/main.ts)
- shell：[src/components/app-shell.ts](../../../../src/components/app-shell.ts)
- router：[src/router/index.ts](../../../../src/router/index.ts)
- store：[src/store/index.ts](../../../../src/store/index.ts)
- contracts：[src/contracts/types.ts](../../../../src/contracts/types.ts)
- theme：[src/theme/index.ts](../../../../src/theme/index.ts)
- i18n：[src/i18n/index.ts](../../../../src/i18n/index.ts)
- pages：[src/pages/](../../../../src/pages/)

## 关联基础 skill

- Vanilla TS/Vite/strict：[tauri2-frontend](../../tauri2-frontend/SKILL.md)
- IPC/event facade：[tauri2-ipc](../../tauri2-ipc/SKILL.md)
- 原生窗口/dialog/tray：[tauri2-desktop-integration](../../tauri2-desktop-integration/SKILL.md)
