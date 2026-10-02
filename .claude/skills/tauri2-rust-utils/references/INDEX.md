# tauri2-rust-utils references

本 skill 是基于本仓库 Rust 内部工具层提炼的领域指南，不复制新的上游文档。实现中的限制、错误和测试定义了可复用契约。

## 本仓库实现

- AES-GCM：[src-tauri/src/utils/cryptox.rs](../../../../src-tauri/src/utils/cryptox.rs)
- 有界文件 I/O：[src-tauri/src/utils/filex.rs](../../../../src-tauri/src/utils/filex.rs)
- HTTP transport：[src-tauri/src/utils/httpx.rs](../../../../src-tauri/src/utils/httpx.rs)
- JSONL 日志：[src-tauri/src/utils/logx.rs](../../../../src-tauri/src/utils/logx.rs)
- 模块入口：[src-tauri/src/utils/mod.rs](../../../../src-tauri/src/utils/mod.rs)

## 关联基础 skill

- 错误、State、阻塞 I/O：[tauri2-rust-backend](../../tauri2-rust-backend/SKILL.md)
- SQLite snapshot/rollback：[tauri2-storage-sqlite](../../tauri2-storage-sqlite/SKILL.md)
- 构建与跨平台验证：[tauri2-build-release](../../tauri2-build-release/SKILL.md)
