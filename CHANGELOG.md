# Changelog

## 2.1.0 — 2026-07-11

- 新增 macOS 12+ Universal 应用与 DMG 候选构建，覆盖 Apple Silicon 与 Intel；未签名候选仅作为 GitHub Actions artifact。
- 保留 Windows x64 NSIS 与便携 ZIP，并保持 `com.gaos6e.codexusage` 标识和现有数据/WebView 偏好连续。
- macOS 使用 `com.gaos6e.chronolume`，增加标准菜单、Command 快捷键、关闭后 Dock 重开、系统主题与平台原生数据目录。
- GitHub Actions 增加 Windows/macOS 测试矩阵、Universal slice/Info.plist/DMG/启动 smoke 验证，以及受保护环境控制的签名发布门禁。
- 正式 macOS Release 仍要求 Developer ID 签名、公证、staple、Gatekeeper 验证和外部 Mac 真机验收。

## 2.0.2 — 2026-07-11

- 总览不再显示“部分会话缺少完整 Token 或生命周期记录”的顶部提示；内部 `partial` 数据状态和可验证统计口径保持不变。

## 2.0.1 — 2026-07-11

- 产品品牌由 Codex Usage 统一更名为 Chronolume。
- 应用标题、EXE、安装目录、NSIS 安装器、便携包、快捷方式、导出文件名、文档和 GitHub 仓库采用同一名称。
- 分析库迁移到 `%LOCALAPPDATA%\Chronolume\v2\chronolume-v2.sqlite3`；首次启动自动移动 2.0.0 数据库及 WAL sidecar，不重新索引或丢失设置。
- 浏览器偏好键迁移到 `chronolume.*` 命名空间；保留稳定的内部 bundle identifier，以确保安装升级和 WebView 偏好连续。

## 2.0.0 — 2026-07-11

- 全面迁移到 Tauri 2 + Rust + React + TypeScript + Vite。
- 新增流式 JSONL 增量索引、完整换行安全偏移、断点续传、归档移动与解析器版本重建。
- 支持 GPT-5.6 动态模型、`turn_context.payload.model`、旧版 `session_meta.payload.model` 与 `model_provider`。
- 新增 `state_5.sqlite` 安全元数据读取和 `logs_2.sqlite` rowid 高水位；不读取日志正文。
- 新增 WAL SQLite 分析库、90 天事件保留和永久会话/每日/模型/工具汇总。
- 新增成本估算、本地价格增删改/恢复、官方价格差异预览和无 JSONL 重读的重算。
- 新增 Dashboard、工作区、会话、模型、工具热力图、诊断、导出、主题、字体缩放和中英文界面。
- 收紧隐私边界：标题、提示词、回复、工具参数、命令正文、文件路径和代码内容不进入分析库。
- 新增真实 4.17 GB 数据 benchmark、隐私测试、增量/重放/DST/定价/查询/前端交互测试。
- 完整统一浅色、暗色与系统主题，原生 Windows 标题栏同步主题；所有界面字号改为可缩放 `rem` 层级并提高小字可读性。
- 设置页改为宽屏满宽对齐，修复 Hero 指标和自定义日期控件对齐，支持关闭数据提示并隐藏页面滚动条轨道。
- 工作区列表默认不展示项目；新增可搜索的“其他工作区”多选界面，所选项目作为项目页与全局筛选快捷项持久化。
- 修复内置价格版本更新后历史事件未重算及日汇总主键收敛冲突；`custom` provider 精确命中官方模型时采用 OpenAI 参考价，内部 `codex-auto-review` 不再触发缺价告警，未知模型不猜价。
- 优化总览 Hero 与峰值日排版；默认 Token 活跃热力图与趋势图改为左窄右宽同排布局。周/月/年分别展示最近 7/30/365 天，采用紧凑 1×7、5×6、7×53 布局；年度默认定位最新日期，隐藏滚动条并支持鼠标拖动。Token、活跃时间使用分位色阶和完整浅色主题配色。
- 模型页默认按版本与强度层级降序，成本均值改为每百万总 Token；隐藏 `unknown`，补充 `gpt-5.3-codex` 官方价与退役 `gpt-5.2-codex` 的可见同层级计价回退。
- Top 工具限制为 10 项并改为固定高度滚动列表。

2.0 是全新应用，不迁移 1.x 设置、缓存或备份。
