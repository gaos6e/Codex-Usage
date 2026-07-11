# 性能基线与验收

## 证据边界

- 下列已有数字全部来自 Windows 真机与真实 `~/.codex` 数据，不能外推为 macOS 结论。
- macOS GitHub runner 只用于编译、测试、Universal 包静态检查和无 `~/.codex` 启动 smoke；2026-07-11 的成功 run `29150578335` 使用 `macos-14-arm64` image `20260629.0180.1`，完成双架构构建、DMG 与启动验证，但没有加载真实用户数据，不能称为真实用户性能。
- 当前尚无 Apple Silicon 或 Intel Mac 的真实用户数据 benchmark。该项属于正式发布前的外部真机验收，不足以证明前不得写“macOS 性能已达标”。

测试日期为 2026-07-10，Windows 机器、Release 构建、真实 `%USERPROFILE%\.codex` 只读数据。首次编译时间不计入应用指标；benchmark 数据库仅写入 `%LOCALAPPDATA%\Chronolume\benchmarks`，不进入 Git。

## 数据规模

首轮完整 benchmark 有 1,798 个 JSONL，计划连同两个 SQLite 来源共 1,800 个、4,171,435,011 字节。任务结束安全复核时源数据已自然增长到 1,797 个活动 JSONL（4,030,388,189 字节）和 2 个归档 JSONL（4,946,810 字节）；`state_5.sqlite` 为 12,996,608 字节，`logs_2.sqlite` 为 134,643,712 字节。增长来自当前 Codex 运行，应用和 benchmark 始终只读源目录。

## 1.x 历史基线

旧 Electron 1.0.5 在独立临时缓存目录测得：

| 场景 | 结果 |
| --- | ---: |
| 全量重建 | 14,322 ms |
| 同进程热刷新 | 127.6 ms |
| 新进程加载持久缓存并查询 | 190.5 ms |
| 旧便携目录总大小 | 571,505,297 字节 |
| 旧 EXE | 222,962,176 字节 |
| 旧安装器 | 128,092,160 字节 |

该实现依赖较小的旧缓存且没有 2.0 的事件库、永久汇总、定价和完整诊断，因此只作迁移前参考，不能与 2.0 首次完整索引直接等同。

## 可重复命令

```powershell
Set-Location .\src-tauri

cargo run --release --features benchmarks --bin usage-benchmark -- `
  "$HOME\.codex" `
  "$env:LOCALAPPDATA\Chronolume\benchmarks\fresh.sqlite3"

cargo run --release --features benchmarks --bin query-benchmark -- `
  "$env:LOCALAPPDATA\Chronolume\benchmarks\fresh.sqlite3"

cargo run --release --features benchmarks --bin incremental-benchmark -- `
  "$HOME\.codex" `
  "$env:LOCALAPPDATA\Chronolume\benchmarks\fresh.sqlite3"
```

`usage-benchmark` 拒绝覆盖已有数据库；`incremental-benchmark` 只读源目录，只更新显式指定的分析库。内存通过 100 ms 间隔采样 Release 进程；启动通过轮询 Tauri 主窗口句柄并结合首个 Dashboard 查询测量。

## 2.0 最终结果

| 指标 | 实测 | 目标 | 结论 |
| --- | ---: | ---: | --- |
| 首次完整索引（样本 A） | 41,578 ms，382,131 条结构化记录 | 后台、可恢复 | 通过 |
| 首次完整索引（独立样本 B） | 33,550 ms | 同上 | 通过 |
| 导入期间 Dashboard | 首次 500 ms 轮询即查询成功 | UI 立即可用 | 通过 |
| 真实普通增量 | 365.65 ms；3 个变更源、40,800 字节、错误 0 | ≤ 1,000 ms | 通过 |
| 常用 SQL/聚合查询 | 180 样本；P50 6.32 ms / P95 8.77 ms / max 18.59 ms | P95 ≤ 150 ms | 通过 |
| 打开 SQLite + 首个 30 天 Dashboard | 13.82 ms | 启动总计 ≤ 1,500 ms | 通过 |
| Release 主窗口 | 141.99 ms | 启动总计 ≤ 1,500 ms | 通过 |
| 最终安装版主窗口 | 125.89 ms | 同上 | 通过 |
| 最终便携版主窗口 | 89.88 ms | 同上 | 通过 |
| 首次导入峰值工作集 | 46.64 MB（私有 40.70 MB） | ≤ 180 MB | 通过 |
| 完整索引后工作集 | 58.4 MB（私有 35.31 MB） | ≤ 180 MB | 通过 |
| 窗口刚就绪工作集 | 22.79 MB | ≤ 180 MB | 通过 |
| 分析库 | 首轮 162,541,568 字节；最终 v2 162,709,504 字节 | 记录 | 通过 |
| SQLite quick check | `ok` | 必须通过 | 通过 |
| 解析失败 | 0 | 0 | 通过 |

首轮导入期间有 17 个正在创建的仅元数据 rollout 暴露出空活跃集合 `SUM` 返回 `NULL` 的边界问题。修复为显式 `COALESCE(..., 0)` 并加入回归测试后，真实增量从 17 个重复错误降为 0；最终 Release 样本的 `runErrors`、`sourceErrors`、解析失败和文件错误均为空/0。

最终打包后又用同一 Release benchmark 追赶 5 个活动源、13,123,809 个新增字节和 2,853 条结构化记录，耗时 562.46 ms，仍为 0 解析失败/文件错误且 integrity 为 `ok`；这验证了小追加和较大积压两种真实增量都低于 1 秒。

首次导入、取消、断点续传、截断/替换/归档、解析器升级和幂等由 Rust 集成测试覆盖。UI 线程不执行文件或 SQLite 工作；所有趋势和表格均由后端预聚合/分页，生产前端最大入口 chunk 为 240.49 kB，Recharts 独立懒加载 chunk 为 356.80 kB。

本轮主题、字体、工作区选择和定价修订后，再次对持续增长的真实生产分析库执行 180 次查询：打开 SQLite 加首个 30 天 Dashboard 为 15.46 ms，P50 10.18 ms、P95 12.33 ms、max 21.45 ms；最终 Release 主窗口 108.17 ms，便携版 95.70 ms。均远低于原定上限，新增工作区目录查询和价格重算标记没有引入持续查询回归。

以上所有硬性目标均达标，没有通过减少 Token 分类、跳过历史来源或降低时间边界精度换取结果。
