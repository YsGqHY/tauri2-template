# tauri2-desktop-integration references

本 skill 是基于本仓库窗口、托盘和 dialog 实现提炼的领域指南，不复制新的上游文档。实际 capability、command 和事件代码是行为真相。

## 本仓库实现

- 主窗口配置：[src-tauri/tauri.conf.json](../../../../src-tauri/tauri.conf.json)
- 主窗口 capability：[src-tauri/capabilities/default.json](../../../../src-tauri/capabilities/default.json)
- 子窗口 capability：[src-tauri/capabilities/children.json](../../../../src-tauri/capabilities/children.json)
- child window：[src-tauri/src/child_windows.rs](../../../../src-tauri/src/child_windows.rs)
- 窗口 commands：[src-tauri/src/commands/windows.rs](../../../../src-tauri/src/commands/windows.rs)
- tray：[src-tauri/src/tray.rs](../../../../src-tauri/src/tray.rs)
- 前端 child service：[src/services/child-window.ts](../../../../src/services/child-window.ts)
- native dialog：[src/services/native-dialogs.ts](../../../../src/services/native-dialogs.ts)

## 关联基础 skill

- capability/CSP/插件授权：[tauri2-config-permissions](../../tauri2-config-permissions/SKILL.md)
- event 与错误 contract：[tauri2-ipc](../../tauri2-ipc/SKILL.md)
- 页面 cleanup：[tauri2-vanilla-app-architecture](../../tauri2-vanilla-app-architecture/SKILL.md)
