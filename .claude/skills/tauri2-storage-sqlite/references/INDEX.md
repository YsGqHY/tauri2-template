# tauri2-storage-sqlite references

本 skill 是基于本仓库 Foundation Desktop 实现提炼的领域指南，不复制新的上游文档。代码是行为真相，入口 skill 负责流程、边界和验证方式。

## 本仓库实现

- 初始化与连接：[src-tauri/src/storage/mod.rs](../../../../src-tauri/src/storage/mod.rs)
- schema 与迁移：[src-tauri/src/storage/schema.rs](../../../../src-tauri/src/storage/schema.rs)
- 设置与偏好：[src-tauri/src/storage/data.rs](../../../../src-tauri/src/storage/data.rs)
- 路径切换与回滚：[src-tauri/src/storage/paths.rs](../../../../src-tauri/src/storage/paths.rs)
- 统计：[src-tauri/src/storage/stats.rs](../../../../src-tauri/src/storage/stats.rs)
- 测试：[src-tauri/src/storage/tests.rs](../../../../src-tauri/src/storage/tests.rs)

## 关联基础 skill

- Rust command、State、阻塞 I/O：[tauri2-rust-backend](../../tauri2-rust-backend/SKILL.md)
- IPC 与错误：[tauri2-ipc](../../tauri2-ipc/SKILL.md)
- 前端 Settings 页面：[tauri2-vanilla-app-architecture](../../tauri2-vanilla-app-architecture/SKILL.md)
- 原子文件 helper：[tauri2-rust-utils](../../tauri2-rust-utils/SKILL.md)
