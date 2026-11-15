import { execFileSync } from 'node:child_process';
import { writeFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { resolve } from 'node:path';

// 分类总结由发布者核对最终代码；完整提交由 Git 生成，避免漏记修复或回退。
export function renderReleaseNotes({
  tag,
  previous,
  repository,
  summary,
  commits,
}) {
  const base = `https://github.com/${repository}`;
  return `# Pi Desktop ${tag}\n\n比较范围：${previous ?? '首次发布'} → ${tag}，共 ${commits.length} 个提交。\n\n${summary.trim()}\n\n## 全部提交\n\n${commits.map(({ hash, subject }) => `- [${hash.slice(0, 7)}](${base}/commit/${hash}) ${subject.replace(/[\\`*_[\]<>]/g, '\\$&')}`).join('\n')}\n\n## 完整差异\n\n${previous ? `[${previous}...${tag}](${base}/compare/${previous}...${tag})` : `[首次发布源码](${base}/tree/${tag})`}\n\n## 安装与限制\n\n未签名构建：Windows NSIS 安装器、macOS .app.tar.gz、Linux Debian 包；架构见附件名称，不提供 macOS Universal 包。\n\n应用不捆绑 Pi 或 Node.js，请先安装运行环境。Windows SmartScreen / macOS Gatekeeper 可能提示或阻止运行；未进行代码签名或 macOS 公证。Linux 包不适用于所有发行版。三平台流水线检查与打包不替代原生交互验收，具体范围见仓库验证文档。\n`;
}

// 使用参数数组调用 Git，不依赖 Shell，兼容 Windows、macOS 和 Linux。
export function generateReleaseNotes(tag, repository) {
  if (!/^v\d+\.\d+\.\d+$/.test(tag)) throw new Error('仅支持正式版本标签');
  if (!/^[\w.-]+\/[\w.-]+$/.test(repository))
    throw new Error('仓库必须为 owner/repo');
  const git = (...args) =>
    execFileSync('git', args, { encoding: 'utf8' }).trim();
  git('rev-parse', '--verify', `${tag}^{commit}`);
  const ancestors = git('rev-list', '--parents', '-n', '1', tag).split(' ');
  let previous = null;
  // 从全部父提交寻找最近的正式版本祖先，排除当前标签及同提交的别名。
  const tags = git('tag', '--merged', tag, '--sort=-version:refname')
    .split('\n')
    .filter((value) => /^v\d+\.\d+\.\d+$/.test(value) && value !== tag);
  let distance = Infinity;
  for (const candidate of tags) {
    if (git('rev-parse', `${candidate}^{commit}`) === ancestors[0]) continue;
    const count = Number(git('rev-list', '--count', `${candidate}..${tag}`));
    if (count < distance) {
      previous = candidate;
      distance = count;
    }
  }
  const range = previous ? `${previous}..${tag}` : tag;
  const log = git('log', '--reverse', '--format=%H%x09%s', range);
  const commits = log
    ? log.split('\n').map((line) => {
        const index = line.indexOf('\t');
        return { hash: line.slice(0, index), subject: line.slice(index + 1) };
      })
    : [];
  // 从标签提交读取总结，工作区未提交的内容不能混入已发布版本。
  const summary = git('show', `${tag}:docs/releases/${tag.slice(1)}.md`);
  const version = JSON.parse(git('show', `${tag}:package.json`)).version;
  if (`v${version}` !== tag) throw new Error('标签与版本号不一致');
  return renderReleaseNotes({ tag, previous, repository, summary, commits });
}

if (
  process.argv[1] &&
  resolve(process.argv[1]) === fileURLToPath(import.meta.url)
) {
  const [tag, repository, output] = process.argv.slice(2);
  if (!output)
    throw new Error(
      '用法：node scripts/release-notes.mjs <tag> <owner/repo> <output>',
    );
  writeFileSync(output, generateReleaseNotes(tag, repository), 'utf8');
}
