# 冷启动模型目录验证记录

## 修复范围

- 桌面桥接在 `session_start` 中等待 `modelRegistry.refresh({ allowNetwork: false })`，避免 RPC 读取尚未收敛的可用模型快照；不发起生成或远端目录发现，不重选模型或重置强度。
- 模型面板区分加载、失败和空目录；失败保留当前选择并支持就地重试，重试前恢复搜索焦点，防止错误区卸载时整个菜单失焦关闭。
- 同运行实例和配置版本合并目录读取，忽略过期结果；启动读取期间新建草稿接收最终目录，恢复的本地草稿继承已读取目录。
- 新增桥接、状态与组件回归，扩充现有浏览器控件任务；未新增独立测试命令或套件。

## 本机执行结果

平台：macOS arm64。最终代码版本执行：

| 命令 | 结果 |
| --- | --- |
| `npm run check` | 0 错误，0 警告 |
| `npm test` | 62 文件，442 通过，0 失败 |
| `node --test scripts/desktop.test.mjs` | 3 通过，0 失败，0 忽略 |
| `npm run build` | 通过 |
| `npx prettier --check src tests "*.ts" "*.js" "*.json" "src-tauri/*.json" "src-tauri/capabilities/*.json" src-tauri/bridge/desktop.mjs` | CI 全范围格式检查通过 |
| `git diff --check` | 通过 |
| `cargo fmt --manifest-path src-tauri/Cargo.toml --check` | 通过 |
| `cargo clippy --manifest-path src-tauri/Cargo.toml --locked --all-targets -- -D warnings` | 通过 |
| `cargo test --manifest-path src-tauri/Cargo.toml --locked` | 89 通过，0 失败，1 个既有忽略 |
| `CI=1 npm run test:browser -- --output=/tmp/pi-model-final-browser` | 再次全量 120 通过，0 失败 |
| `npm run tauri -- build --ci --no-sign -- --locked` | macOS arm64 原生 release 编译和未签名 `.app` 打包通过 |
| 最小 `PATH=/usr/bin:/bin` 下以已解析 Cargo 路径执行 `cargo test --manifest-path src-tauri/Cargo.toml --locked platform::tests::installed_pi_starts_with_gui_path -- --ignored --exact` | 专项 1 通过，0 失败，0 忽略；其余 89 项被过滤，不代替全量 |

Rust 工具初始不在 PATH，补充本机 `$HOME/.cargo/bin` 后执行上述命令。既有忽略为 `platform::tests::installed_pi_starts_with_gui_path`；续验已在最小 GUI PATH 下显式执行并通过，验证真实安装的 Pi 能完成版本探测，不发送消息、不加载用户会话。本次未修改或新增忽略。续验重新执行类型检查、442 项单元测试、3 项桌面测试、CI 全范围格式检查、前端构建、Rust fmt/clippy/全量测试以及 120 项浏览器测试，全部通过；原生打包命令也重新执行其 beforeBuildCommand 检查与前端构建。未安装、启动或替换用户正在使用的应用。

本次 macOS 未签名产物：`src-tauri/target/release/bundle/macos/Pi Desktop.app`（约 7.49 MiB）。未执行签名或公证，不代表可直接发布。续验日志位于 `/tmp/pi-model-verify-{frontend,rust,browser,native,installed}.log`，临时日志不作为跨机器永久产物。

浏览器初次缺少 Playwright Chromium，安装后执行。期间另一任务同时运行浏览器测试，占用 1421 端口并清理共享 `test-results`，出现端口冲突与 trace ENOENT；未终止其他进程、未关闭检查。待端口空闲后以 CI 原有单 worker 模式及独立临时产物目录执行全量，120 项通过。曾发现本次重试按钮卸载导致面板失焦关闭的真实失败，已修复并补充焦点断言；修复后重新执行完整验证，不以单用例重跑代替全量。

实际检查 `test-results/model-catalog-startup.png`：当前模型及其他模型同时显示，分组、搜索、选中标记和面板边界正常；原有浏览器用例继续覆盖窄窗口与深色。

## 边界

自动化模型接口均使用可控替身，不读取或修改用户认证文件，不向真实服务商发送消息。本次没有在真实原生客户端中验证冷启动。macOS arm64 原生打包已通过，但当前主机为 macOS，没有 Windows/Linux 原生执行环境；工作区修改尚未提交，远端 CI 不能验证该未提交版本。本次未擅自提交、推送或触发发布，Windows/Linux CI 及原生打包仍待在对应 runner 上验证。发布还需真实客户端冷启动验收、其余平台检查与打包，不能据本机结果宣称三平台原生验证完成。

工作区同时存在会话书签相关改动；本次未编辑其实现文件，最终全量测试包含该工作区当时的测试集。
