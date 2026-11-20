# 当前会话代码变更采集验证记录

## 状态与范围

最终实现版本在本机 Windows 的全部必需自动化命令通过。Node.js 为 v24.13.0，Cargo 为 1.93.1。浏览器使用 Playwright 配置的系统 Edge，无用例重试。

macOS、Linux 的 CI，以及三平台原生桌面交互和打包尚未执行，状态为待验证；本记录不代表三平台原生验收或发布完成。工作区原有文件预览相关修改保留，本轮全量检查包含这些已有修改。

## 修正的行为

- 文件清单来自当前会话 edit/write 的结构化目标，不再将整个项目同期变化作为会话修改。
- 工具开始、结束采集受限快照；首次前内容与末次后内容连续且无已知冲突时计算净差异。覆盖写入按真实前后内容统计，恢复原文不产生净变更。
- 同文件跨会话涉及、同路径工具重叠、工具间外部变化、成功写入结果不符或消失、失败后目标变化、未结束工具及无法比较的目标保留原因，不进入精确汇总，不提供误导性的净差异正文。
- 失败且未改变磁盘的工具不产生虚假文件记录。成功 write 的开始快照已等于结束内容时，标记原始内容无法确认，避免因事件到达过晚而静默漏记。
- 明确记录的 build 等目录目标不再因目录名被排除；超限、项目外、符号链接及不可读目标保留路径和原因。
- bash、脚本及未适配工具只报告可能存在未记录修改，不猜测修改归属。事件不是工具执行屏障，同文件外部编辑仍可能影响观测结果，不能承诺原子归因。
- 新版采集版本为 3，覆盖状态及归属状态结构化传输。旧版工作区记录继续可读并标注归属未验证，不重建旧历史。

## 最终版本执行结果

各套件串行执行，避免重型浏览器、前端 worker 和原生进程调度同时争用资源。环境设置为 `CI=1`、`RUST_TEST_THREADS=1`、`CARGO_INCREMENTAL=0`；Vitest 的 `VITEST_MAX_THREADS` / `VITEST_MAX_FORKS` 为 2，最小值为 1。未删除测试、增加忽略项、修改超时或弱化断言。

| 实际命令 | 结果 | 通过 / 失败 / 忽略 |
| --- | --- | --- |
| `npm run check` | 通过，0 错误、0 警告 | 不适用 |
| `npm test` | 64 个测试文件通过 | 467 / 0 / 0 |
| `node --test scripts/desktop.test.mjs` | 通过 | 3 / 0 / 0 |
| `node --test scripts/release-notes.test.mjs` | 通过 | 3 / 0 / 0 |
| `npm run test:browser` | 128 项、单 worker、无重试 | 128 / 0 / 0 |
| `npm run build` | 通过，330 个模块转换完成 | 不适用 |
| `cargo fmt --manifest-path src-tauri/Cargo.toml --check` | 通过 | 不适用 |
| `cargo clippy --manifest-path src-tauri/Cargo.toml --locked --all-targets -- -D warnings` | 通过 | 不适用 |
| `cargo test --manifest-path src-tauri/Cargo.toml --locked` | 通过；默认忽略项另行执行，见下文 | 119 / 0 / 1 |
| `npx prettier --check src tests "*.ts" "*.js" "*.json" "src-tauri/*.json" "src-tauri/capabilities/*.json" src-tauri/bridge/desktop.mjs scripts/release-notes.mjs scripts/release-notes.test.mjs` | 通过 | 不适用 |
| `git diff --check` | 通过 | 不适用 |

Rust 原有忽略项 `platform::tests::installed_pi_starts_with_gui_path` 已使用最终构建的测试二进制单独执行：

```text
src-tauri/target/debug/deps/pi_air_lib-673a980a3082142d.exe --ignored --exact platform::tests::installed_pi_starts_with_gui_path
```

该子进程的 PATH 仅保留系统目录，结果为 1 通过、0 失败、0 忽略；其余 119 项被这个定向命令过滤，不是跳过全量检查。上表全量测试中的唯一默认忽略项因此已补充实际验证。

最终自动化命令日志保留在本机临时目录 `C:/Users/Administrator/AppData/Local/Temp/pi-air-attribution-verified-checks/`。

## 回归覆盖与视觉核对

新增 Rust 用例覆盖会话之间不同文件隔离、同文件冲突、工具间外部编辑、净差异与恢复、失败或未结束工具、未知工具覆盖限制、单文件及内容预算、项目外路径、覆盖写入结果校验、事件过晚及工具事件缺失。符号链接针对性用例在 Unix 条件下执行，本机 Windows 未执行该 Unix 专用用例，需 macOS/Linux CI 补充。

前端用例验证不确定目标不计入汇总、不打开差异，以及结构化覆盖状态下的不完整空结果。浏览器用例验证就地说明的键盘操作、草稿保留、不可打开状态、汇总、窄窗口无横向溢出；原有轮次导航、右侧审阅、焦点恢复和布局测试同属完整套件并通过。

实际查看以下四张新增截图：

- `test-results/attribution-light-1500.png`
- `test-results/attribution-light-480.png`
- `test-results/attribution-dark-1500.png`
- `test-results/attribution-dark-480.png`

截图中主题、文字层次、间距与现有工具栏一致，原因说明能够换行，未发现重叠、意外截断或页面横向溢出。禁用目标保持现有禁用样式，说明仍可通过信息按钮访问。未进行完整对比度测量、缩放矩阵或原生自动主题切换验收，不据此宣称符合 WCAG。

## 中间检查问题与处理

初轮 Rust 检查存在一个仍按全工作区扫描构造数据的旧用例失败，已补齐工具事件并保留删除、二进制、持久化和会话隔离断言。两个会话调度用例在前端与 Rust 并行执行时超时；串行套件、限制 worker 后全量通过，未放宽其超时或断言。

初轮编译还报告 Windows 增量缓存目录“拒绝访问”。最终关闭增量编译后，Clippy 与 Rust 测试正常完成，未再出现该警告。中间因实现继续修正而中止的执行不计为最终通过证据。

npm 仍提示用户级镜像配置键不再受支持；前端测试的既有 MessageView.edit 夹具还会产生非响应式绑定开发警告。它们未导致命令失败，本次未调整全局 npm 配置或无关编辑交互。
