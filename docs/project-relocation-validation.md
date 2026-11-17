# 项目目录自动关联与合并验证记录

状态：最终代码的 Windows 本机全量自动化验证通过；三平台原生验收待完成。

## 范围

可靠的原生目录身份、唯一候选及提交前复核成立时，后台关联并合并已登记的重复项目。身份缺失或不确定时人工选择。保留旧 ID、别名与默认偏好，保留双方会话及独立草稿，不改写 Pi 历史文件。

旧任务结束后更新，目标项目和其他项目任务继续；持久化目标、双方身份及退休阶段，等待可取消，取消后不立即重排。事务失败保留日志和队列，重复请求通过被吸收 ID 解析。

手动页采用 350 ms 输入检查、一个提交按钮、按需内嵌浏览、中性合并说明、折叠诊断。支持明暗主题、窄窗口、中文及长路径；保留输入、焦点和阅读位置。

## 环境

Windows，Node.js 24.13.0、npm 11.6.2、Rust/Cargo 1.93.1，系统 Edge。最终检查顺序执行，`CI=1` 使用仓库现有的单 worker 浏览器配置，`RUST_TEST_THREADS=1` 串行执行 Rust 用例，各调度用例内部仍检查并发行为。`CARGO_INCREMENTAL=0` 避免本机增量目录访问拒绝。不修改断言、增加重试或放宽超时。

## 最终结果

以下命令均在最终代码上执行，退出码均为 0。计数不包含早期失败或中断的尝试。

| 命令                                                                                                                                                    |                  通过 | 失败 | 忽略／补充                            |
| ------------------------------------------------------------------------------------------------------------------------------------------------------- | --------------------: | ---: | ------------------------------------- |
| `npm run check`                                                                                                                                         |              1 项检查 |    0 | Svelte／TypeScript 0 错误、0 警告     |
| `npm test`                                                                                                                                              | 461 个测试、64 个文件 |    0 | 0 忽略                                |
| `node --test scripts/desktop.test.mjs`                                                                                                                  |                     3 |    0 | 0 忽略                                |
| `node --test scripts/release-notes.test.mjs`                                                                                                            |                     3 |    0 | 0 忽略                                |
| `npm run test:browser`                                                                                                                                  |                   127 |    0 | 0 忽略；单 worker，5.6 分钟           |
| `npm run build`                                                                                                                                         |              1 项构建 |    0 | Vite 生产构建                         |
| `cargo fmt --manifest-path src-tauri/Cargo.toml --check`                                                                                                |              1 项检查 |    0 | Rust 格式                             |
| `cargo clippy --manifest-path src-tauri/Cargo.toml --locked --all-targets -- -D warnings`                                                               |              1 项检查 |    0 | 全目标静态分析                        |
| `cargo test --manifest-path src-tauri/Cargo.toml --locked`                                                                                              |                   110 |    0 | 1 个既有忽略项，见下行                |
| `cargo test --manifest-path src-tauri/Cargo.toml --locked -- --include-ignored`                                                                         |                   111 |    0 | 0 忽略，实际执行本机 Pi GUI PATH 用例 |
| `node scripts/verify-process-pool.mjs C:/Users/Administrator/AppData/Roaming/npm/node_modules/@earendil-works/pi-coding-agent/dist/cli.js`              |      1 项真实协议验证 |    0 | 目录、原历史及工具 cwd 断言通过       |
| `npx prettier --check src tests src-tauri/bridge/desktop.mjs scripts/verify-process-pool.mjs docs/interaction.md docs/project-relocation-validation.md` |              1 项检查 |    0 | 本轮全部相关文件                      |

另外执行 CI 范围的格式检查：

```text
npx prettier --check src tests "*.ts" "*.js" "*.json" "src-tauri/*.json" "src-tauri/capabilities/*.json" src-tauri/bridge/desktop.mjs scripts/release-notes.mjs scripts/release-notes.test.mjs scripts/verify-process-pool.mjs docs/interaction.md docs/project-relocation-validation.md
```

Rust 默认忽略的 `platform::tests::installed_pi_starts_with_gui_path` 已通过额外的 `--include-ignored` 全套运行执行，不将默认忽略计作通过；main 和 doc tests 均为 0 个测试。`git diff --check` 同时检查通过。

查看了当前明暗主题的 760 px 窄窗口截图：原位置、输入与合并预览层级清楚，完整长路径换行、仅一个主提交按钮，默认不展示技术诊断。系统主题切换与原生 WebView 仍见下方待验证范围。

完整命令输出保存在 `%TEMP%/pi-air-project-checks/`；前一版记录在 `%TEMP%/pi-air-project-checks-before-merge/`。本次排查记录在 `%TEMP%/pi-air-project-checks-check-unit-rust-clippy-rust-test/`、`%TEMP%/pi-air-project-checks-rust-test-browser/` 等独立目录；针对性验证通过不替代最终全套通过。

## 排查与覆盖

- Windows 短路径与完整路径属于同一目录，合并边界核对规范路径，偏好与排序不因拼写不同遗漏。运行时记住启动时的路径等价关系，原目录不存在后仍能冻结、查询和退休原实例；不建立旧路径到新项目的永久重定向。
- Windows 会拒绝改名子进程正在使用的当前目录。对应池状态回归将实际子进程置于父目录，再模拟原池登记，跨平台覆盖路径消失后的实例定位与真实退休；未跳过该测试。
- 浏览器 IPC 夹具必须返回独立配置快照，模拟真实 Tauri 序列化，不与页面共享可变对象；自动合并断言检查入口去重、无人工提交、无管理页面、草稿及焦点保留。
- 保存失败后须等待 DOM 更新再恢复按钮焦点；默认提示不展示系统错误与诊断编号，展开详情后可查看。
- Rust 事务测试用目录占用配置文件目标，跨平台制造真实原子替换失败，验证内存和已持久化阶段不变，恢复写入后合并最新偏好且仅提交一次。
- 自动证据失效、目标自身有待处理事务、歧义归属、取消后的重排抑制、归档、模型回退、会话去重、旧路径复用及共享 DTO 均有回归覆盖。
- 曾遇到浏览器准备阶段的页面加载超时，后续使用单独的请求诊断确认页面可正常加载；未放宽既有 120 秒准备期限或测试超时，最终仍需完整套件通过。

## 真实 Pi 与原生验收

真实 Pi 验证检查原会话文件恢复、工具 cwd 使用新目录、历史继续追加和原会话头不变，使用临时目录，不请求在线模型。

macOS、Linux 真实文件系统身份、监听、进程恢复，以及三平台原生 WebView、缩放和系统主题交互仍待验收。三平台 CI、原生打包与发布未执行，不将本机自动化结果视为三平台完成。
