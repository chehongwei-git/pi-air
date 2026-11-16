# pi-air 改名验证

产品展示名称、npm/Rust package 名称统一为 `pi-air`，Rust 库名为 `pi_air_lib`。界面、窗口、HTML 标题、启动错误、发布说明生成器和 CI 产物规则同步更新。版本仍为 `2026.11.15`。

保留应用标识 `dev.pi.desktop`、配置目录、`pi-desktop-sidebar` / `pi-desktop-workspace-v1` 存储键与 `pi-desktop-*-v1` 桥接协议。历史发布说明与既往验证记录中的旧产物名称保持原样；仓库地址未变。

## 执行环境与结果

本轮针对最终 `pi-air` 代码执行，平台为 Windows x64；Node.js v24.13.0、npm 11.6.2、rustc / cargo 1.93.1。此前 `piAir` 阶段的结果不计入本轮。

| 实际命令 | 结果 |
| --- | --- |
| `npm run check` | 通过，0 错误、0 警告 |
| `npm test` | 63 个文件、452 个用例通过，0 失败、0 跳过 |
| `node --test scripts/desktop.test.mjs` | 3 通过、0 失败、0 跳过 |
| `node --test scripts/release-notes.test.mjs` | 3 通过、0 失败、0 跳过 |
| `npm run test:browser` | 系统 Edge，124 通过、0 失败、0 跳过 |
| `npm run build` | 通过 |
| `cargo fmt --manifest-path src-tauri/Cargo.toml --check` | 通过 |
| `cargo clippy --manifest-path src-tauri/Cargo.toml --locked --all-targets -- -D warnings` | 通过，退出码 0；增量缓存目录收尾出现 Windows 拒绝访问警告 |
| `cargo test --manifest-path src-tauri/Cargo.toml --locked` | 92 通过、0 失败、1 个既有环境用例忽略；main/doc-tests 均为 0 用例 |
| `npx prettier --check src tests "*.ts" "*.js" "*.json" "src-tauri/*.json" "src-tauri/capabilities/*.json" src-tauri/bridge/desktop.mjs scripts/release-notes.mjs scripts/release-notes.test.mjs` | 通过 |
| `npm run tauri -- build --ci --no-sign -- --locked` | Windows release 与 NSIS 打包通过 |
| `npm run test:native` | 窗口标题 `pi-air`、responding=true、关闭退出码 0 |
| `git diff --check` | 通过 |

默认忽略的 `platform::tests::installed_pi_starts_with_gui_path` 已额外通过：直接执行本轮 Cargo 生成的测试程序 `src-tauri/target/debug/deps/pi_air_lib-4f09b5a537bcaeba.exe platform::tests::installed_pi_starts_with_gui_path --ignored --exact --nocapture`，仅对子进程将 PATH 设为 Windows System32；1 通过、0 失败、0 忽略，92 个其他用例按名称过滤。该验证只探测真实 Pi 版本，不发送模型消息。

## 产物与界面核对

- 本轮 Windows 可执行文件：`src-tauri/target/release/pi-air.exe`。
- 本轮未签名安装包：`src-tauri/target/release/bundle/nsis/pi-air_2026.11.15_x64-setup.exe`，构建日志约 2.85 MiB。
- 已检查浏览器本轮浅色、深色与窄屏截图：`test-results/slash-commands-light.png`、`test-results/slash-commands-dark.png`、`test-results/settings-narrow.png`；侧栏显示 `pi-air`，品牌文字无截断、错位，复用原有布局与样式。
- 名称与锁文件一致性断言通过；源代码、构建配置、清单和当前 README 中无 `piAir` / `piair` / `pi_desktop_lib` / `Pi Desktop` 品牌残留。旧协议键、临时目录前缀和仓库地址按兼容约定保留。
- 发布说明渲染预览标题为 `# pi-air v2026.11.15`。未生成或发布实际 Release，未修改标签。

## 平台范围与限制

必需自动化命令在 Windows x64 全部退出成功；Rust 全量命令报告的 1 个既有忽略项已另行执行通过。npm 提示既有用户配置项未来不受支持；Rust 编译曾提示增量缓存目录拒绝访问，未影响命令成功，均未通过关闭检查规避。

macOS、Linux 本轮未执行原生构建、交互或 CI；已同步 macOS `pi-air.app` 归档路径与 CI 上传/下载附件规则，仍需三平台 CI 验证。未运行安装器覆盖旧版本，不宣称已验证升级安装兼容；保留 identifier 与存储键不等同于升级安装验收。未签名、未公证，未发布。

本机完整命令日志保存在临时目录 `C:\Users\ADMINI~1\AppData\Local\Temp\pi-air-checks-m46tc0`；临时日志不是跨机器永久产物。本文件保留命令、数量、平台和范围，浏览器截图及原生冒烟输出位于未纳入 Git 的 `test-results/`。
