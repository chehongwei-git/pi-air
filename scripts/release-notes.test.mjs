import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { mkdtempSync, mkdirSync, writeFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { test } from 'node:test';
import { renderReleaseNotes } from './release-notes.mjs';

const script = fileURLToPath(new URL('./release-notes.mjs', import.meta.url));

// 临时仓库覆盖真实 Git 行为，不修改工作区标签或依赖在线 GitHub。
function fixture(run) {
  const cwd = mkdtempSync(join(tmpdir(), 'pi-release-'));
  const git = (...args) =>
    execFileSync('git', args, { cwd, encoding: 'utf8' }).trim();
  try {
    git('init');
    git('config', 'user.name', 'Release Test');
    git('config', 'user.email', 'release@example.test');
    const commit = (subject) => git('commit', '--allow-empty', '-m', subject);
    const version = (value) => {
      mkdirSync(join(cwd, 'docs', 'releases'), { recursive: true });
      writeFileSync(
        join(cwd, 'package.json'),
        JSON.stringify({ version: value }),
      );
      writeFileSync(
        join(cwd, 'docs', 'releases', `${value}.md`),
        '## 功能总结\n\n- 中文功能',
      );
      git('add', '.');
      commit(`release: ${value}`);
    };
    const generate = (tag) => {
      execFileSync(process.execPath, [script, tag, 'owner/repo', 'notes.md'], {
        cwd,
        stdio: 'pipe',
      });
      return execFileSync('git', ['show', `${tag}:package.json`], {
        cwd,
        encoding: 'utf8',
      });
    };
    run({ cwd, git, commit, version, generate });
  } finally {
    rmSync(cwd, { recursive: true, force: true });
  }
}

test('完整明细保留回退、中文和链接，转义 Markdown 提交标题', () => {
  const result = renderReleaseNotes({
    tag: 'v2026.11.15',
    previous: 'v2026.11.7',
    repository: 'owner/repo',
    summary: '## 功能总结\n- 已核实',
    commits: [{ hash: 'a'.repeat(40), subject: 'Revert "feat: [功能](url)"' }],
  });
  assert.match(result, /共 1 个提交/);
  assert.ok(result.includes('Revert "feat: \\[功能\\](url)"'));
  assert.ok(result.includes('/commit/' + 'a'.repeat(40)));
  assert.match(result, /compare\/v2026.11.7\.\.\.v2026.11.15/);
  assert.match(result, /安装与限制/);
});

test('真实仓库选择最近祖先而非最大版本，包含分支提交并忽略工作区说明', () => {
  fixture(({ cwd, git, commit, version }) => {
    version('2026.11.6');
    git('tag', 'v2099.1.1');
    version('2026.11.7');
    git('tag', 'v2026.11.7');
    const branch = git('branch', '--show-current');
    git('checkout', '-b', 'feature');
    commit('feat: 分支功能');
    git('checkout', branch);
    commit('fix: 主线修复');
    git('merge', '--no-ff', 'feature', '-m', 'Merge feature');
    version('2026.11.15');
    git('tag', '-a', 'v2026.11.15', '-m', '中文摘要');
    writeFileSync(
      join(cwd, 'docs', 'releases', '2026.11.15.md'),
      '未提交错误说明',
    );
    const result = execFileSync(
      process.execPath,
      [
        '--input-type=module',
        '-e',
        `import { generateReleaseNotes } from ${JSON.stringify(new URL('./release-notes.mjs', import.meta.url).href)}; console.log(generateReleaseNotes('v2026.11.15', 'owner/repo'));`,
      ],
      { cwd, encoding: 'utf8' },
    );
    assert.match(result, /v2026.11.7 → v2026.11.15，共 4 个提交/);
    assert.match(result, /feat: 分支功能/);
    assert.match(result, /fix: 主线修复/);
    assert.match(result, /Merge feature/);
    assert.match(result, /中文功能/);
    assert.doesNotMatch(result, /未提交错误说明/);
  });
});

test('首次发布无基线；缺少总结、无效标签和版本不一致均阻止生成', () => {
  fixture(({ cwd, git, version, generate }) => {
    version('2026.11.15');
    git('tag', 'v2026.11.15');
    generate('v2026.11.15');
    git('tag', 'v2026.11.16');
    assert.throws(() => generate('v2026.11.16'));
    assert.throws(() => generate('--bad'));
    git('rm', 'docs/releases/2026.11.15.md');
    git('commit', '-m', 'remove summary');
    git('tag', '-f', 'v2026.11.15');
    assert.throws(() => generate('v2026.11.15'));
    assert.ok(cwd);
  });
});
