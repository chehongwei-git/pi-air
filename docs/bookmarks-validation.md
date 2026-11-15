# 完整提问目录验证记录

## 当前实现

- 对话可阅读后自动整理当前分支完整提问摘要，正常状态不提供“加载更早提问”。后台只传摘要，正文仍在点击定位时按需补齐。
- Rust Projection 在用户消息投影时建立摘要索引，查询直接切索引，不重新读取 JSONL、不调用 Pi 全量快照，不传图片、正文引用、助手或工具信息。
- 新增受限 `bookmarks` action，每项仅 `id` 和最多 100 码点的 `label`；每批最多 256 项，约 128KiB 预算。历史重建递增修订，拒绝旧游标。
- 每个会话视图拥有 BookmarkDirectory，串行自动读完，批次间等待 40ms 让出事件循环；完成后无轮询。隐藏时停止后续批次，在途结果可进入原会话缓存；关闭悬停目录不清理缓存。卸载释放缓存，重连或会话路径变化重建。
- 正式用户消息通过已有本地消息投影增量合入，不因助手文字 token 请求目录；重复提问以 ID 区分。
- 超过 100 项时采用固定 38px 行高窗口化，最多挂载 24 个条目按钮；正文保持普通文档流。键盘跨窗口移动时先把焦点停在稳定入口，防止被卸载行造成意外关闭。
- 加载与定位错误分离，失败停止自动重试并保留已读摘要，显式重试。慢请求在展开后延迟 250ms 显示轻量状态。

## 悬停展开性能优化

- 首次使用才创建有界面板，之后收起保留节点；未使用目录的会话不承担列表 DOM 开销。历史身份改变清理准备状态。
- 悬停等待 150ms 调整为 60ms，离开宽限 180ms；显示前准备目标虚拟窗口、测量视口并写入滚动位置。展开只改变透明度与 4px 位移，120ms 展开、90ms 收起；不动画布局属性、不常驻 will-change。隐藏立即 inert / aria-hidden，减少动画偏好下无过渡。
- 本机 Chromium 万条摘要夹具采样：修改前四次为 166、161.8、165.8、164.8ms；最终全量运行四次为 78.9、82、81.6、81.9ms。指标为 pointerenter 到首个 expanded 且 opacity > 0 的动画帧，不是完整动画耗时，也不是原生帧率保证。
- MutationObserver 记录重复三次展开/收起新增和移除节点均为 0；首次仍需创建有界面板。自动化持续断言重复节点复用、请求数不增加、快速移回、关闭 inert、减少动画、键盘恢复及原有阅读锚点。
- 全量测试曾发现预先创建所有隐藏目录导致文字定位歧义，调整为首次打开准备面板，保留原测试断言。截图明确等待 opacity=1 后记录最终状态，避免把淡入中间帧误判成文字重影；最终窄屏深色截图已查看。
- 最终优化版本日志为 `/tmp/pi-hover-*.log`，浏览器截图在 `/tmp/pi-hover-browser-final`。首次额外原生打包因组合命令超时被中止，之后单独完整执行同一打包命令通过，不把超时计为通过。

## 最终代码版本本机全量检查

平台：macOS ARM64。以下命令均退出码 0；没有新增 skip、ignore 或削弱检查。

| 命令 | 结果 |
| --- | --- |
| `npm run check` | 0 errors、0 warnings |
| `npm test` | 63 文件，446 通过、0 失败 |
| `node --test scripts/desktop.test.mjs` | 3 通过、0 失败、0 跳过 |
| `npm run test:browser -- --workers=1 --output=/tmp/pi-hover-browser-final` | 全量 123 通过、0 失败 |
| `npm run build` | 生产构建通过 |
| `npx prettier --check src tests *.ts *.js *.json src-tauri/*.json src-tauri/capabilities/*.json` | 全部格式通过 |
| `cargo fmt --manifest-path src-tauri/Cargo.toml --check` | 通过 |
| `cargo clippy --manifest-path src-tauri/Cargo.toml --locked --all-targets -- -D warnings` | 通过 |
| `cargo test --manifest-path src-tauri/Cargo.toml --locked` | 90 通过、0 失败、1 既有 ignored |
| `npm run tauri -- build --ci --no-sign -- --locked` | macOS ARM64 未签名 .app 打包通过 |

Cargo 命令通过将已有 `$HOME/.cargo/bin` 加入当前 Shell PATH 执行。Rust 既有忽略项 `platform::tests::installed_pi_starts_with_gui_path` 另以最小 PATH 执行 `PATH=/usr/bin:/bin "$HOME/.cargo/bin/cargo" test --manifest-path src-tauri/Cargo.toml --locked platform::tests::installed_pi_starts_with_gui_path -- --ignored`，1 通过、0 失败。默认全量中的 ignored 标记未更改，不将其混入默认通过数。

原生打包重新执行生产构建及 Rust release 编译，生成 `src-tauri/target/release/bundle/macos/Pi Desktop.app`；不将旧产物计为新版本通过。未签名、未公证，也未启动客户端操作真实会话。

## 性能与交互覆盖

- `src/lib/bookmark-directory.test.ts`：串行读完整目录、重复启用不请求、在途请求合并、后台暂停恢复、分支失效丢弃迟到页、失败停止、显式重试、万条目录请求数。
- 万条摘要的合成测试每批 256 条，共 40 个请求；读完后推进模拟时钟仍是 40 个请求，没有轮询。批次调度的理论等待约 1.6 秒，不是实际磁盘、IPC 或原生 WebView 耗时承诺。
- Rust `bookmark_index_is_bounded_and_revision_checked` 使用一万项索引，断言批次长度、序列化体积、100 码点摘要、仅两个字段、末页与旧修订/非法游标；真实 mock RPC 二十轮生命周期中校验目录 action 返回与历史正文一致的 ID，且没有正文引用。
- `tests/browser/bookmarks.spec.ts` 使用万条目录验证未悬停就自动读取、40 请求后停止、展开/助手输出不请求、正文消息数不增加、目录按钮最多 24 个、Home/End 和跨窗口焦点恢复。完整摘要在内存中，缓存空间仍随提问总数增长，不承诺恒定内存。
- 原有宽屏浅色、480px 深色与 125% 缩放覆盖继续通过：几何位置不移动、内部滚动不带动正文、跨页点击定位、草稿保留、流式阅读保护、错误重试及会话切换。
- 人工查看全量 Playwright 生成的窄屏深色和大目录截图，未见正文/输入框被挤压或目录越界。长列表键盘测试首次暴露被卸载焦点导致关闭，修复后重新执行全量验证。

## 平台范围与剩余限制

- `.github/workflows/check.yml` 已有 Windows、macOS、Linux 全量测试和原生打包矩阵，新单元/浏览器/Rust 用例由现有命令自动发现，无需新增套件命令。
- Windows/Linux CI、打包及三个平台的原生 WebView/触控板/触控交互未在本次环境执行；macOS 打包不代表原生交互通过。整体状态是本机验证通过、跨平台发布验收待验证，不宣称可以发布。
- 目录内存随提问数增长，超长会话仍需实际设备测量；本次验证资源边界和请求/DOM 数量，不声称达到特定启动延迟或帧率。
- 取消定位停止后续正文分页及最终滚动，不撤销已在途的一页历史插入。深历史定位仍需补齐中间正文，其 DOM 和耗时随历史增长。
- 前次每 50 条完整消息分页读取目录的实现已被专用摘要 action 替代；原提示词召回的 promptHistory 接口保持不变。
