# tauri2-subprocess-lifecycle references

本 skill 是基于本仓库受限 subprocess 实现提炼的领域指南，不复制新的上游文档。代码和测试是生命周期契约的行为真相。

## 本仓库实现

- worker、reader、限制与历史：[src-tauri/src/subprocess.rs](../../../../src-tauri/src/subprocess.rs)
- Unix/Windows 进程组：[src-tauri/src/process_group.rs](../../../../src-tauri/src/process_group.rs)
- lifecycle gate 与 JoinHandle：[src-tauri/src/state.rs](../../../../src-tauri/src/state.rs)
- IPC facade：[src-tauri/src/commands/subprocess.rs](../../../../src-tauri/src/commands/subprocess.rs)
- event payload：[src-tauri/src/events.rs](../../../../src-tauri/src/events.rs)
- 前端页面：[src/pages/subprocess/index.ts](../../../../src/pages/subprocess/index.ts)

## 关联基础 skill

- async、线程、锁与错误：[tauri2-rust-backend](../../tauri2-rust-backend/SKILL.md)
- command/event contract：[tauri2-ipc](../../tauri2-ipc/SKILL.md)
- 构建与跨平台验证：[tauri2-build-release](../../tauri2-build-release/SKILL.md)
