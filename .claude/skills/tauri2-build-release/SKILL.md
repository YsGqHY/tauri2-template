---
name: tauri2-build-release
description: tauri2-template 的构建与发布指南。说明 dev/build 命令链路、release profile 的体积优化含义、打包目标与图标、Windows 工具链的实测要求、国内网络的镜像加速，以及本项目已踩过并验证的坑。
---

# 构建与发布

本项目构建链路已完整验证通过（Rust 侧 `cargo build` 成功链接，产出 13MB debug exe）。本文档记录的工具链结论**均为本机实测**，不是文档推测。

---

## 1. 命令与链路

| 命令 | 实际发生了什么 |
|------|--------------|
| `pnpm tauri dev` | 执行 `beforeDevCommand`（`pnpm dev` 起 Vite）→ 编译 Rust debug → 开窗口加载 `devUrl` |
| `pnpm tauri build` | 执行 `beforeBuildCommand`（`pnpm build` 出 `dist/`）→ 编译 Rust release → 按 `bundle.targets` 打包 |
| `pnpm build` | 仅前端：`tsc && vite build` → `dist/` |
| `cargo check` | 仅类型检查，**不链接**（在 `src-tauri/` 下执行） |
| `cargo build` | 完整编译含链接（在 `src-tauri/` 下执行） |
| `pnpm tauri info` | 环境诊断，排查工具链问题第一步 |

**`cargo check` 通过不代表能构建**。它跳过链接阶段，而链接器缺失/版本问题恰是 Windows 上最常见的故障。验证工具链完整性必须跑 `cargo build`。本项目正是用这一步确认 MSVC 2017 可用。

dev 模式下 Rust 代码改动会自动重编译并重启窗口；前端改动走 Vite HMR，不重启。

---

## 2. Windows 工具链要求（实测结论）

Tauri 官方推荐 VS 2022 Build Tools，但本机实测组合：

| 组件 | 本机版本 | 结论 |
|------|---------|------|
| Rust | 1.98.1 stable-x86_64-pc-windows-msvc | 必须是 **MSVC** toolchain，不是 GNU |
| MSVC Build Tools | 2017（14.16.27023） | **可用**，完成链接无报错 |
| Windows SDK | 10.0.17763.0 / 10.0.19041.0 | 可用 |
| WebView2 运行时 | 149.0.4022.62 | 已装（Win10/11 通常自带） |

结论：**不必为了本项目升级到 VS 2022**。若将来引入依赖较新 C++ 标准的 crate 可能需要升级，届时再处理。

WebView2 是**运行时依赖**，不只是构建依赖。目标机器缺它应用无法启动。Windows 11 与较新 Win10 自带；分发给旧系统需在安装包里带 bootstrapper（`bundle.windows.webviewInstallMode` 配置）。

---

## 3. 依赖下载加速（国内网络）

本项目构建需拉取数百个 crate。官方源实测 101KB/s，清华 TUNA 镜像 1.9MB/s，**约 19 倍差距**。

cargo 镜像已配置在**用户级** `~/.cargo/config.toml`（不在仓库内，换机器需重新配置）：

```toml
[source.crates-io]
replace-with = 'tuna'

[source.tuna]
registry = "sparse+https://mirrors.tuna.tsinghua.edu.cn/crates.io-index/"

[net]
git-fetch-with-cli = true
```

rustup 安装工具链本身也会走官方源。装 Rust 前先设环境变量，否则极易超时挂起：

```bash
export RUSTUP_DIST_SERVER="https://mirrors.tuna.tsinghua.edu.cn/rustup"
export RUSTUP_UPDATE_ROOT="https://mirrors.tuna.tsinghua.edu.cn/rustup/rustup"
```

---

## 4. release profile 的体积优化

[src-tauri/Cargo.toml](../../../src-tauri/Cargo.toml#L28) 已配好：

```toml
[profile.release]
codegen-units = 1    # 单编译单元，优化更彻底但编译更慢
lto = true           # 链接时优化，跨 crate 内联
opt-level = 3        # 最高速度优化（体积优先可改 "s" 或 "z"）
panic = "abort"      # panic 直接终止，不生成 unwind 表
strip = true         # 剥离符号表
```

**`panic = "abort"` 有行为影响**：release 下 panic 不会 unwind，`std::panic::catch_unwind` 失效。若业务依赖捕获 panic 兜错，必须改掉这行。debug 构建不受影响（仍是 unwind）。

体积优先时把 `opt-level = 3` 改 `"z"`，通常能再省一些，代价是运行速度。

---

## 5. 打包目标与图标

```json
"bundle": { "active": true, "targets": "all", "icon": [...] }
```

`"all"` 会产出当前平台所有格式。Windows 上是 `.msi`（WiX）与 `.exe`（NSIS）。只要一种可收窄以提速：

```json
"targets": ["nsis"]
```

产物位置：`src-tauri/target/release/bundle/<格式>/`。

图标必须齐全，缺失会导致打包失败。换图标不要手工逐个替换，用 CLI 从一张源图生成全套：

```bash
pnpm tauri icon path/to/icon.png     # 建议源图 1024x1024 带透明通道
```

它会覆盖 `src-tauri/icons/` 下全部文件，包含 `.ico`（Windows）、`.icns`（macOS）、多尺寸 png 与 Windows Store 用的 Square*Logo。

---

## 6. 跨平台构建的现实限制

**Tauri 不支持真正的交叉编译**。Windows 上无法产出 macOS/Linux 安装包（webview 与系统库依赖本地）。多平台分发需在各平台各自构建，或用 CI（GitHub Actions 的 `tauri-action` 是常规做法）。

Linux 构建额外需要系统库（webkit2gtk 等）；macOS 需 Xcode 命令行工具与签名证书。这也是 [src-tauri/Cargo.lock](../../../src-tauri/Cargo.lock) 里存在 GTK/Linux 相关 crate 的原因 —— 它们在 Windows 上不参与编译，只是出现在依赖图里。

---

## 7. 本项目已踩过的坑（勿重复排查）

**rustup 安装挂起**：从官方源下载组件时会长时间无输出直至超时，且中断后留下残缺工具链 —— 症状是 `cargo` 可用但 `rustc` 报 `missing manifest`。修复：

```bash
rm -f ~/.rustup/downloads/*.partial
rm -f ~/.rustup/update-hashes/*
# 设置镜像环境变量后
rustup toolchain install stable-x86_64-pc-windows-msvc --force --no-self-update
```

**`rustc --version` 本身会触发重新同步**：工具链残缺时这条命令会静默开始下载 6 个组件并卡住，看起来像命令挂了。别以为是命令有问题。

**4 个传递依赖落后于最新版**：`generic-array 0.14.7`、`toml 0.8.2`、`toml_datetime 0.6.3`、`toml_edit 0.20.2`。已查明是 Tauri 上游约束（`toml` 经 `system-deps`，`generic-array` 经 `sha2` → `tauri-codegen`），**不是本项目问题**。不要用 `--precise` 强行绕过，会破坏依赖图一致性。

**`pnpm-workspace.yaml` 不要随手删**：它是 pnpm 发布时间门禁（minimumReleaseAge）自动生成的例外记录，内容是允许 `plugin-opener@2.7.0` 通过门禁。删除后重新解析依赖可能让版本回退到 2.6.0。

**验证升级不能只跑 `--frozen-lockfile`**：该模式跳过解析阶段，而发布时间门禁只在解析阶段生效，会给出假绿灯。

---

## 8. 版本基准（2026-09-30 实测）

| 组件 | 版本 | 备注 |
|------|------|------|
| tauri | 2.12.0 | 最新稳定版 |
| tauri-build | 2.7.0 | 最新稳定版 |
| tauri-plugin-opener | 2.7.0 | Rust 与 JS 侧已对齐 |
| @tauri-apps/api | 2.12.0 | 最新 |
| @tauri-apps/cli | 2.12.0 | 最新 |
| TypeScript | 7.0.2 | 跨大版本升级，经隔离实测通过 |
| Vite | 8.3.1 | 最新 |
| wry / tao | 0.57.0 / 0.37.1 | webview / 窗口底层 |

crates.io 索引里存在 `tauri 3.0.0-alpha.3`，是**预发布版**，本项目刻意未采用。查询最新稳定版时需过滤 alpha/beta/rc。

---

## 9. 反例

❌ 用 `cargo check` 通过就认定构建没问题 —— 它不链接。

❌ 没配镜像直接在国内网络首次构建 —— 可能卡数十分钟或超时。

❌ 手工逐个替换 `src-tauri/icons/` 下的图标 —— 用 `pnpm tauri icon`。

❌ 期待在 Windows 上打出 macOS 安装包 —— 不支持，需各平台分别构建。

❌ release 下依赖 `catch_unwind` 兜错 —— `panic = "abort"` 使其失效。

❌ 提交构建产物 —— `target/`（[src-tauri/.gitignore](../../../src-tauri/.gitignore#L3)）与 `dist/`（[.gitignore](../../../.gitignore#L11)）已忽略，勿强制添加。

---

## 10. 与其他层的协调

- `bundle` / `identifier` / `productName` 等字段含义见 `.claude/skills/tauri2-config-permissions/SKILL.md`。
- `panic = "abort"` 对错误处理的影响见 `.claude/skills/tauri2-rust-backend/SKILL.md`。
- 前端构建产物路径由 `frontendDist` 约定，见 `.claude/skills/tauri2-frontend/SKILL.md` 第 9 节。
- 改名涉及的 5 处同步点见根 [CLAUDE.md](../../../CLAUDE.md) 的「改名 / 业务化指南」。

## 官方 references

对应的 Tauri v2 官方原文已同步到 [`references/`](references/)，索引、上游路径、commit 和许可见 [`references/INDEX.md`](references/INDEX.md)。优先查阅 `start/prerequisites.mdx`、`distribute/index.mdx`、`distribute/windows-installer.mdx` 与 `concept/size.mdx`。
