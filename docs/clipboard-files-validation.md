# 剪贴板文件粘贴验证记录

## 范围与实现

桌面输入框粘贴优先调用专用 `paste_files` 命令，使用 clipboard-rs 0.3.5（禁用图片编解码依赖，启用 Wayland）读取系统文件列表。Windows 使用 CF_HDROP，macOS 使用文件 URL，Linux 使用 URI 列表，并补充 GNOME 专用列表回退。宿主不接受前端提供的文件路径、不执行系统命令、不修改剪贴板、不执行剪切移动；文件分类、目录引用及图片预算复用既有 file_import。

无原生文件列表时回退 WebView 图片或普通文本；文本保留选区插入、收纳和撤销。文件结果通过已有导入队列归并，导入期间禁止发送，切换上下文后作废迟到结果。普通文件不自动读取正文，发送时仍是路径引用。

新增测试由既有 Vitest、Playwright 和 Cargo 入口自动发现，无新增套件或 CI 命令。覆盖普通文件粘贴、WebView files 为空、去重、移除、发送、读取失败保留草稿、文本回退与撤销、切换上下文、URL 解码、复制／剪切列表顺序及非法地址。

## 最终代码版本自动化验证

环境：macOS arm64。本机已执行以下完整命令，全部退出码为 0。日志保存在本机临时目录 `/tmp/pi-clipboard-validation/`（0–8.log、status.json），不纳入仓库。运行浏览器测试时设置进程环境 CI=true；Rust 工具链通过用户 cargo bin 目录加入当前进程 PATH。

| 命令 | 结果 |
| --- | --- |
| `npm run check` | 0 错误，0 警告 |
| `npm test` | 63 个文件、452 项通过，0 失败、0 跳过 |
| `node --test scripts/desktop.test.mjs` | 3 通过，0 失败、0 跳过 |
| `npx prettier --check src tests "*.ts" "*.js" "*.json" "src-tauri/*.json" "src-tauri/capabilities/*.json" src-tauri/bridge/desktop.mjs` | 通过 |
| `npm run test:browser` | 124 通过，0 失败、0 跳过 |
| `npm run build` | 通过 |
| `cargo fmt --manifest-path src-tauri/Cargo.toml --check` | 通过 |
| `cargo clippy --manifest-path src-tauri/Cargo.toml --locked --all-targets -- -D warnings` | 通过 |
| `cargo test --manifest-path src-tauri/Cargo.toml --locked` | 93 通过，0 失败，1 项既有环境依赖测试忽略；主程序与文档测试各 0 项 |
| `cargo test --manifest-path src-tauri/Cargo.toml --locked platform::tests::installed_pi_starts_with_gui_path -- --ignored --exact` | 上述忽略项单独执行：1 通过，0 失败、0 忽略 |
| `git diff --check` | 通过 |

已查看本轮浏览器测试生成的 `test-results/file-attachments-light.png`、`file-attachments-dark-narrow.png`、`file-attachments-dark-zoom.png`：复用附件卡片，明暗主题、480px 窄窗及 200% 缩放下详情、移除与发送操作保持可见；附件横向溢出采用已有滚动容器，不新增弹窗。

## 待验证边界

- 自动化 UI 使用模拟宿主 IPC，不代表实际系统文件管理器与 WebView 的剪贴板验收；本轮未修改用户剪贴板来进行原生复制实验。
- macOS Finder 实际复制／截图粘贴、Windows 资源管理器、Linux GNOME/KDE（X11 与 Wayland）的原生实机操作仍待验证。Wayland 受 compositor 的剪贴板协议支持限制，应检查目标桌面环境。
- Windows／Linux 编译、三平台 CI、原生安装包与发布验收尚未执行；不宣称完成三平台兼容验证或发布。
- 系统剪贴板从用户粘贴到宿主实际读取间可能被其他应用替换，本实现立即发起读取以缩短窗口，但不承诺跨进程原子快照。
- 完整 WCAG 审核未执行。

状态：实现和 macOS 本机必需自动化检查已通过；三平台原生交互及发布验收待验证。
