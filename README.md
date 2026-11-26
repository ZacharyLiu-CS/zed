> [!IMPORTANT]
> Remove this line to confirm you've reviewed this PR before submitting.

# Zed Custom (`custom-sidebar`)

基于上游 Zed 官方稳定版维护的个人定制发行版，重点重构了 **Agent Panel 侧边栏** 与 **长对话阅读/跳转体验**，并通过 GitHub Actions 自动追更上游稳定版与推送客户端自动更新。

<p align="center">
  <img src="./docs/images/custom-agent-sidebar.png" alt="Custom Agent Panel Preview" width="680" />
</p>

### 定制增强特性

- **紧凑高信息密度 Agent 侧边栏（参考 T3 Code 风格）**
  - **彩色文件夹缩写徽章**：按项目名自动生成 2 位大写缩写（如 `adt_projects` → `AP`、`dop-scripts` → `DS`、`ceph-agent` → `CA`），为不同文件夹分配高区分度独立配色，亮色/暗色主题独立调校、切换主题即时刷新。
  - **清晰的字体层级**：Project 标题跟随 Settings 中的 `ui_font_size` 实时缩放，Session 标题自动精确缩小 2 个字号（`-2px`），项目与会话主次分明。
  - **清晰的 Agent 类型与图标识别**：右侧以透明底精致图标直观显示启动该会话的 Agent 图标与名称，自动支持 `claude`、`codex`、`cursor`、`codebuddy-code`、`zed`、`gemini`、`opencode`、`copilot` 以及任意自定义 ACP（如 `tclaude`、`tcodex`、`remote-claude` 自动继承对应品牌图标与配色并保留自定义名称）。
- **Agent 对话阅读与时间统计增强**
  - **用户输入与 AI 回复清晰分区**：用户输入改为**右对齐主题色微染气泡**（带 `You` 与发送时间），AI 输出保持**左对齐全宽**且每段回复起点自动显示 **Agent 品牌图标 + 名称 + 开始时间与执行耗时**，一眼分清谁说的话、回答从哪里开始。
  - **每轮与整会话执行时间统计**：自动记录每轮提问发送时间、AI 开始时间与执行耗时（运行中实时跳秒，并在顶部栏统计整个会话累计耗时），通过 `KeyValueStore` 持久化，重启 Zed 后历史耗时依然保留。
  - **一键隐藏思考与命令执行**：对话顶部栏提供 `Hide steps` / `Steps hidden` 开关（同时支持 Action `agent::ToggleHideThinkingAndTools`），一键折叠所有 `Thinking` 块与已完成的工具调用 / shell 命令，只保留「你的提问 ↔ Agent 回复」（等待确认、运行中或出错的步骤始终保持可见）。
  - **左侧零遮挡进度刻度尺导航**：对话包含多轮提问时，对话左侧独立列显示波浪刻度尺仪表盘（绝不遮挡正文，窄宽度下同样清爽），高亮当前阅读进度；鼠标悬停任意刻度在右侧弹出该轮问题、发送时间、耗时与回复摘要卡片，点击一键跳转。
- **与官方 Zed 无缝并存 + 自动追更**
  - 独立打包为 **`Zed Custom.app`**（`CFBundleIdentifier: dev.zed.Zed-Custom`），与 `/Applications/Zed.app` 并存互不覆盖，同时共享 `stable` 渠道配置、线程历史与官方远端 SSH `zed-remote-server`（无需本机 `zig` 交叉编译）。
  - GitHub Actions 每天自动检测上游最新稳定版 Tag（`vX.Y.Z`）、自动 rebase、构建签名并发布到 [Releases](https://github.com/ZacharyLiu-CS/zed/releases)；客户端自动检测并原地静默更新 `/Applications/Zed Custom.app`。

### 安装方式（macOS Apple Silicon）

1. 前往 [Releases](https://github.com/ZacharyLiu-CS/zed/releases) 下载最新的 `Zed-Custom-aarch64.dmg`。
2. 将 `Zed Custom.app` 拖入 `/Applications`（不会覆盖官方 `/Applications/Zed.app`）。
3. 首次打开前执行一次移除隔离标记，并先退出正在运行的官方 `Zed.app`：
   ```bash
   xattr -dr com.apple.quarantine "/Applications/Zed Custom.app"
   ```
4. 后续有上游新稳定版或自定义更新时，应用会自动检测并在右下角提示 **Restart to Update**。

---

# Zed

[![Zed](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/zed-industries/zed/main/assets/badge/v0.json)](https://zed.dev)
[![CI](https://github.com/zed-industries/zed/actions/workflows/run_tests.yml/badge.svg)](https://github.com/zed-industries/zed/actions/workflows/run_tests.yml)

Welcome to Zed, a high-performance, multiplayer code editor from the creators of [Atom](https://github.com/atom/atom) and [Tree-sitter](https://github.com/tree-sitter/tree-sitter).

---

### Installation

On macOS, Linux, and Windows you can [download Zed directly](https://zed.dev/download) or install Zed via your local package manager ([macOS](https://zed.dev/docs/installation#macos)/[Linux](https://zed.dev/docs/linux#installing-via-a-package-manager)/[Windows](https://zed.dev/docs/windows#package-managers)).

Other platforms are not yet available:

- Web ([tracking discussion](https://github.com/zed-industries/zed/discussions/26195))

### Developing Zed

- [Building Zed for macOS](./docs/src/development/macos.md)
- [Building Zed for Linux](./docs/src/development/linux.md)
- [Building Zed for Windows](./docs/src/development/windows.md)

### Contributing

See [CONTRIBUTING.md](./CONTRIBUTING.md) for ways you can contribute to Zed.

Also... we're hiring! Check out our [jobs](https://zed.dev/jobs) page for open roles.

### Licensing

Zed source code is licensed primarily under GPL-3.0-or-later, with Apache-2.0 components where marked.

License information for third party dependencies must be correctly provided for CI to pass.

We use [`cargo-about`](https://github.com/EmbarkStudios/cargo-about) to automatically comply with open source licenses. If CI is failing, check the following:

- Is it showing a `no license specified` error for a crate you've created? If so, add `publish = false` under `[package]` in your crate's Cargo.toml.
- Is the error `failed to satisfy license requirements` for a dependency? If so, first determine what license the project has and whether this system is sufficient to comply with this license's requirements. If you're unsure, ask a lawyer. Once you've verified that this system is acceptable add the license's SPDX identifier to the `accepted` array in `script/licenses/zed-licenses.toml`.
- Is `cargo-about` unable to find the license for a dependency? If so, add a clarification field at the end of `script/licenses/zed-licenses.toml`, as specified in the [cargo-about book](https://embarkstudios.github.io/cargo-about/cli/generate/config.html#crate-configuration).

## Sponsorship

Zed is developed by **Zed Industries, Inc.**, a for-profit company.

If you’d like to financially support the project, you can do so via GitHub Sponsors.
Sponsorships go directly to Zed Industries and are used as general company revenue.
There are no perks or entitlements associated with sponsorship.
