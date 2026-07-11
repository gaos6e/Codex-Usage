# Chronolume

> Illuminate the rhythm of your work.

Chronolume 是面向 OpenAI Codex 的 Windows 本地活动、用量与成本分析应用。2.0 使用 Tauri 2、Rust、React、TypeScript 与 Vite 构建，直接在后台增量索引 `~/.codex`，不依赖代理或云服务。

## 应用预览

<p align="center">
  <a href="docs/images/chronolume-dashboard.png">
    <img src="docs/images/chronolume-dashboard.png" alt="Chronolume 本地用量总览" width="100%">
  </a>
</p>

<table>
  <tr>
    <td width="50%" align="center">
      <a href="docs/images/chronolume-models.png">
        <img src="docs/images/chronolume-models.png" alt="Chronolume 模型与成本分析" width="100%">
      </a>
      <br>
      <sub><b>模型与成本</b> · 模型分布、缓存命中率与本地价格覆盖</sub>
    </td>
    <td width="50%" align="center">
      <a href="docs/images/chronolume-activity.png">
        <img src="docs/images/chronolume-activity.png" alt="Chronolume 工具与活动分析" width="100%">
      </a>
      <br>
      <sub><b>工具与活动</b> · 每日调用趋势与隐私友好的结构化分类</sub>
    </td>
  </tr>
</table>

## 隐私边界

- 不查询或保存提示词、助手回复、标题、预览、首条用户消息、代码或命令正文。
- 工具参数只在内存中用于“搜索/读取/写入/编辑/执行/其他”分类，分类后立即丢弃。
- 不读取 `auth.json`、ChatGPT 在线配额或账号信息。
- 唯一可选联网能力是用户主动触发的 OpenAI 官方价格检查；更新必须先预览差异再应用。
- CSV、JSON 与 PNG 导出不包含对话内容；结构化导出支持匿名路径和完整路径模式。

详细约束见 [docs/privacy.md](docs/privacy.md)。

## 功能

- 今日、24 小时、7/14/30/90 天、全部与固定/实时自定义范围。
- 工作区、模型提供方、模型和归档状态级联筛选。
- Hero 指标、monotone 渐变趋势图、年度活跃热力图。
- 工作区、会话、模型、定价、工具活动和数据诊断页面。
- 90 天事件明细与永久会话/每日/模型/工具汇总。
- 后台首次导入、真实进度、取消、断点续传、增量同步、重建和修复。
- 中英文、浅色/暗色/系统主题、字体缩放与 reduced-motion。

## 开发

环境要求：Node.js/npm、Rust stable MSVC、Visual Studio Build Tools C++ workload、WebView2 Runtime。

```powershell
npm install
npm run dev
```

运行 Tauri 开发窗口：

```powershell
npm run tauri:dev
```

## 验证

```powershell
npm run typecheck
npm run lint
npm test
npm run build

Set-Location src-tauri
cargo fmt --check
cargo check
cargo clippy --all-targets --all-features -- -D warnings
cargo test
```

真实只读数据 benchmark：

```powershell
Set-Location src-tauri
cargo run --release --bin usage-benchmark -- "$HOME\.codex" "$env:LOCALAPPDATA\Chronolume\benchmarks\fresh.sqlite3"
```

benchmark 只读取 `~/.codex`，分析库写到显式指定路径。目标和实测结果见 [docs/performance.md](docs/performance.md)。

## 构建

```powershell
npm run tauri:build
```

NSIS 安装器与 Release EXE 位于 `src-tauri/target/release`；最终交付同时提供便携 ZIP。产物路径、哈希和 smoke test 见 [docs/packaging.md](docs/packaging.md)。应用数据保存在：

```text
%LOCALAPPDATA%\Chronolume\v2\chronolume-v2.sqlite3
```

2.0 不迁移 1.x 缓存或设置，详见 [docs/migration-and-cleanup.md](docs/migration-and-cleanup.md)。
