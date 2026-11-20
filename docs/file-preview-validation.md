# 对话文件预览验证记录

## 行内代码文件识别收紧

行内代码改为“常见后缀＋明确路径”识别：裸名称仅匹配固定后缀集合或约定文件名（含 `.env`／`.env.*`、`Dockerfile`、`LICENSE` 等）；`FileTarget.line`、`user.name`、`process.env`、`foo.bar` 及其带行号形式保留普通代码样式。明确路径支持未知后缀与无扩展名文件，末段不能为空、`.` 或 `..`。自动识别不查询磁盘，显式 Markdown 链接、网站入口、文件预览 DTO 和宿主命令保持兼容。

新增三个单元／渲染用例覆盖用户原句、误判符号与行号、约定文件名、跨平台明确路径和显式 Markdown 链接；复用现有浏览器套件，检查 `FileTarget.line` 不产生链接或预览请求，并保留真实文件点击、行号定位、重试、焦点恢复和草稿断言。`main.ts:12`、`main.ts:12:3`、`main.ts#L12` 仍定位到预览第 12 行；列号不用于光标定位，行范围仅定位起始行，读取限额仍为 256 KiB。

## 本次识别规则验证（Windows）

最终代码按顺序执行以下全量检查，所有命令退出码均为 0；无新增跳过或忽略：

| 命令 | 结果 |
| --- | --- |
| `npm run check` | 0 错误、0 警告 |
| `npm test` | 64 文件、465 通过，0 失败、0 跳过；80.65 秒 |
| `node --test scripts/desktop.test.mjs` | 3 通过，0 失败、0 跳过 |
| `node --test scripts/release-notes.test.mjs` | 3 通过，0 失败、0 跳过 |
| `CI=1 npm run test:browser` | 127 通过，0 失败、0 跳过；单 worker，5.8 分钟 |
| `npm run build` | 通过 |
| `npx prettier --check src tests '*.ts' '*.js' '*.json' 'src-tauri/*.json' 'src-tauri/capabilities/*.json'` | 通过；Windows 子进程使用等效双引号传入 glob |
| `cargo fmt --manifest-path src-tauri/Cargo.toml --check` | 通过 |
| `CARGO_INCREMENTAL=0 cargo clippy --manifest-path src-tauri/Cargo.toml --locked --all-targets -- -D warnings` | 通过，无警告 |
| `cargo test --manifest-path src-tauri/Cargo.toml --locked` | 111 通过，0 失败、1 既有忽略；main 与文档测试各 0 项 |
| `git diff --check` | 通过 |

新增回归在修改识别实现前为 8 通过、3 失败，确认复现代码符号误判及约定文件名缺失；实现后 11 项相关测试通过，并在上述最终全量套件再次通过。本轮浏览器套件未重试、未中断；已查看 `website-links-wide-light.png`、`website-links-narrow-dark.png` 与 `file-preview-narrow-dark.png`，代码符号无链接下划线、真实链接可见、窄屏换行和预览行高亮正常。

Clippy 沿用上一轮已确认的 Windows 增量缓存权限问题的环境处理，通过 Node 子进程设置 `CARGO_INCREMENTAL=0`，不关闭检查或降低警告级别。Rust 测试保留默认环境，编译收尾仍有一次增量缓存目录“拒绝访问（os error 5）”警告，测试本身全部执行并退出 0；该环境警告不标为已解决。Rust 既有忽略项为 `platform::tests::installed_pi_starts_with_gui_path`，原因为需安装 Pi 并以最小 PATH 单独执行，未计为通过。前端测试保留原有 MessageEdit 非响应式测试夹具提示，npm 的本机旧配置警告亦保留，未调整无关实现。

本轮仅验证 Windows 自动化及浏览器截图；浏览器使用 IPC 模拟，不代表原生 WebView 联调。更新后安装版的实际点击、macOS/Linux 全量检查、三平台原生打包与系统关联仍待验证，交付状态为**待原生／跨平台验证**。

## Windows 盘符链接修复

修复 AI 回复中的 `/C:/…` Markdown 链接直接进入 Windows 文件系统时可能触发 error 123 的问题。普通链接和 `file:///C:/…` 在单次 URL 解码后统一去掉盘符前的斜杠；行号独立传递。真实 Markdown 渲染回归同时发现并修复 DOMPurify 将 `C:/…` 文件路径属性当作自定义协议移除的问题：仅将惰性的 `data-file-path` 标记为无需 URL 协议清洗，实际导航仍使用原有协议限制，宿主继续验证本地路径与项目边界。

Windows 宿主新增非法盘符位置、残留行号冒号、NTFS 数据流及非法文件名字符检查。新增测试覆盖中文空格、百分号只解码一次、正反斜杠、盘符前斜杠、行列号与行号范围、普通 POSIX 路径和编码后的网络路径拒绝；浏览器覆盖 Windows 链接首次失败后重试、行号定位、准确 IPC 参数及关闭后的焦点恢复。不改变预览布局和命令契约。

本次验证平台为 Windows；用户实际点击的原始链接尚未取得，因此上述缺口已复现并修复，但不声称已核实该诊断编号对应的唯一触发路径。

| 命令 | 本轮结果 |
| --- | --- |
| `npm run check` | 通过，0 错误、0 警告 |
| `npm test` | 64 文件、462 用例通过，0 失败 |
| `node --test scripts/desktop.test.mjs` | 3 通过，0 失败、0 跳过 |
| `node --test scripts/release-notes.test.mjs` | 3 通过，0 失败、0 跳过 |
| `CI=1 npm run test:browser` | 127 通过，0 失败、0 跳过；单 worker 全量耗时 6.1 分钟 |
| `npm run build` | 通过 |
| `npx prettier --check src tests '*.ts' '*.js' '*.json' 'src-tauri/*.json' 'src-tauri/capabilities/*.json'` | 通过 |
| `cargo fmt --manifest-path src-tauri/Cargo.toml --check` | 通过 |
| `CARGO_INCREMENTAL=0 cargo clippy --manifest-path src-tauri/Cargo.toml --locked --all-targets -- -D warnings` | 通过，无警告；环境变量通过 Node 子进程传入 |
| `cargo test --manifest-path src-tauri/Cargo.toml --locked` | 111 通过，0 失败、1 既有忽略；main 与文档测试均为 0 项 |
| `git diff --check` | 通过 |

验证过程保留记录：新增 Markdown 回归先复现多余斜杠，修复后继续发现清洗器移除盘符属性，最终相关 8 项单测均通过。中间一次前端全量运行在 60 秒工具时限被中止（当时仍有新增回归失败）；最终代码的完整前端套件使用后台日志等待结束，462 项通过，耗时 97.11 秒。初次 Rust fmt 提示一处换行，已修正并检查通过。首次 Clippy 退出 0，但增量缓存收尾出现“拒绝访问（os error 5）”警告；显式关闭增量缓存后完整 Clippy 无警告通过，不修改检查级别。首次浏览器以默认 4 worker 与 Rust 编译重叠运行，多个无关页面陆续加载超时；中止时日志记录 50 项通过、7 项失败，其余未完成，不计为全量通过。后续使用仓库现有 CI 单 worker 配置，待 Rust 完成后独立执行整个浏览器套件，127 项全量通过；未改超时、断言、重试或跳过设置。

Rust 既有忽略项仍为 `platform::tests::installed_pi_starts_with_gui_path`（需要本机安装 Pi、以最小 PATH 单独执行），未计为通过。已查看本轮宽屏浅色与窄屏深色预览截图，留白、工具栏、源码目标行正常。macOS/Linux CI、三平台原生打包及更新后应用中真实文件链接点击尚未执行，整体仍为**待原生／跨平台验证**；浏览器 IPC 模拟与 Windows Rust 实际文件读取不替代原生 WebView 联调验收。

## 实现范围

- Markdown 本地链接、行内代码路径、文件卡片统一接入现有审阅区域，沿用所属会话工作目录。
- UTF-8 文本限量 256 KiB；签名识别的常见图片限量 10 MiB；Markdown 排版／源码切换；行号定位。
- 文档白名单系统打开，脚本、可执行权限文件、未知格式不直接运行；项目外访问就地确认，网络路径拒绝。
- 预览为磁盘当前内容，不是历史快照；不含 PDF／Office 内嵌渲染、非 UTF-8 转码、大文件全文分页。

## 最终版本本机检查（macOS）

| 命令 | 结果 |
| --- | --- |
| `npm run check` | 通过，0 错误、0 警告 |
| `npm test` | 57 文件、418 用例通过，0 失败 |
| `node --test scripts/desktop.test.mjs` | 3 通过，0 失败、0 跳过 |
| `CI=1 npm run test:browser` | 111 通过，0 失败；IPC 为模拟，不代替原生打开 |
| `npm run build` | 通过；保留主包超过 500 kB 的体积警告 |
| `npx prettier --check src tests '*.ts' '*.js' '*.json' 'src-tauri/*.json' 'src-tauri/capabilities/*.json'` | 通过 |
| `cargo fmt --manifest-path src-tauri/Cargo.toml --check` | 通过 |
| `cargo clippy --manifest-path src-tauri/Cargo.toml --locked --all-targets -- -D warnings` | 通过 |
| `cargo test --manifest-path src-tauri/Cargo.toml --locked` | 88 通过，0 失败、1 既有忽略 |
| `git diff --check` | 通过 |

Rust 忽略项为 `platform::tests::installed_pi_starts_with_gui_path`，原有说明为“需要本机安装 Pi；通过最小 PATH 单独执行”。未删除、增加或绕过测试忽略。本次不将忽略项计为通过。

新增覆盖路径编码／中文／Windows 语法／行号、危险协议／网络路径拒绝、代码块不识别、代码样式链接标签不嵌套、项目外审批、符号链接边界（Unix）、二进制／文本限额／图片签名、脚本系统打开拒绝、迟到读取与重试。浏览器验证真实 Transcript／ReviewLayout 的宽屏浅色与窄屏深色面板、键盘关闭与焦点恢复、草稿保持、所属项目参数和系统打开请求。已检查截图 `test-results/file-preview-wide-light.png` 与 `test-results/file-preview-narrow-dark.png`，未发现溢出、截断或控件错位。

## 预览留白与视觉调整验证

本次仅修改 FilePanel 局部样式与浏览器夹具／断言：统一 24px／16px 水平留白、正文上下间距、分层标题和操作栏、圆角代码阅读区、柔和行定位、Markdown 阅读宽度、图片内边距及反馈区域。不改文件访问接口和其他审阅面板。

最终样式版本已执行：`npm run check`（0 错误、0 警告）、`npm test`（57 文件、418 通过）、`node --test scripts/desktop.test.mjs`（3 通过）、`CI=1 npm run test:browser`（111 通过）、`npm run build`（通过，主包体积警告保留）、上表 Prettier 命令（通过）、Rust fmt／Clippy（通过）、`cargo test --manifest-path src-tauri/Cargo.toml --locked`（88 通过、1 既有忽略）。平台为 macOS；无新增跳过。

首次将前端、浏览器与 Rust 全量套件同时运行时，模型选择大目录和插件页分类测试分别触发 5 秒超时（416 通过、2 失败）。日志显示多组无关测试耗时明显增加；随后取消套件间并行，在相同最终代码上依序执行前端完整检查，418 项全部通过，总耗时从 43.26 秒降至 16.32 秒。未修改超时、断言或忽略项。该次资源争用失败保留记录，不以单独重跑失败用例替代全量验证；后续验收应避免同时运行多套重型测试。

浏览器新增断言覆盖代码、Markdown、图片、错误、项目外授权、长路径的左右留白，以及 150% 缩放下关闭按钮可见和面板不横向溢出。保留原有系统打开、焦点恢复、行号及草稿断言。已人工查看宽屏代码、窄屏深色 Markdown、宽屏错误截图；几何断言覆盖其余状态。对应截图位于 `test-results/file-preview-*.png`。本次不增加原生／跨平台验证声明。

## 待验证范围与交付状态

本机自动化已通过（除上述既有显式忽略）；整体标记为**待原生／跨平台验证**，不宣称全测试零忽略或三平台发布完成。

- Windows、Linux 的 CI、原生打包及本机默认文件关联未在此 macOS 环境执行。
- macOS 原生 WebView、真实 Office／PDF／文件管理器关联打开未实机执行；浏览器仅验证 IPC 请求。
- 三平台仍需验证路径大小写／盘符、系统无关联应用时的错误、原生焦点与缩放；Windows 还需检查脚本和快捷方式拒绝策略。对应平台应运行项目全量清单并完成原生打包验收。
- 规范化与读取之间若有外部进程并发替换文件，当前实现不提供操作系统级文件身份锁定，不应视为对恶意并发文件系统的完整沙箱。
