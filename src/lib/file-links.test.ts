import { describe, expect, it } from 'vitest';
import { codeFileTarget, fileTarget, websiteTarget } from './file-links';
import { markdown } from './markdown';
describe('对话文件入口', () => {
  it('明确裸域名补全 HTTPS，不再产生文件入口', () => {
    for (const value of [
      'btdig.com',
      'nyaa.si',
      'EXAMPLE.COM',
      'www.example.co.uk/search?q=hello#result',
      'example.com:8443/docs/main.ts',
      'example.com/#L12',
    ]) {
      expect(websiteTarget(value)).toBe(new URL(`https://${value}`).href);
      expect(fileTarget(value)).toBeNull();
      expect(codeFileTarget(value)).toBeNull();
    }
  });
  it('歧义文件与显式路径不推断成网站，危险内容不补全', () => {
    for (const value of [
      'main.ts',
      'README.md',
      'foo.bar',
      'archive.zip',
      './btdig.com',
      '/tmp/btdig.com',
      'docs/example.com',
      'C:\\work\\example.com',
      'file:///tmp/example.com',
      '//example.com',
      'https://example.com',
      'user@example.com',
      'javascript:alert(1)',
      'example.com\\evil',
      'example.com\n',
      'example.com:99999',
      '-bad.com',
      'example..com',
    ])
      expect(websiteTarget(value)).toBeNull();
    for (const value of [
      'main.ts',
      'README.md',
      './btdig.com',
      'docs/example.com',
    ])
      expect(codeFileTarget(value)).toEqual({ path: value });
    expect(codeFileTarget('main.ts:12')).toEqual({ path: 'main.ts', line: 12 });
  });
  it('网站保留代码外观与外层链接目标，代码块不转换', () => {
    const root = document.createElement('div');
    root.innerHTML = markdown(
      '`btdig.com` 和 `nyaa.si`、`https://example.com/a`\n\n[网站](example.com/search?q=x)\n\n[`nyaa.si`](https://example.org)\n\n```\nbtdig.com\n```',
    );
    expect(root.querySelector('[data-file-path]')).toBeNull();
    expect(
      root.querySelector('a[href="https://btdig.com/"] code')?.textContent,
    ).toBe('btdig.com');
    expect(
      root.querySelector('a[href="https://nyaa.si/"] code'),
    ).not.toBeNull();
    expect(
      root.querySelector('a[href="https://example.com/a"]'),
    ).not.toBeNull();
    expect(
      root.querySelector('a[href="https://example.com/search?q=x"]'),
    ).not.toBeNull();
    expect(
      root.querySelector('a[href="https://example.org"] code')?.textContent,
    ).toBe('nyaa.si');
    expect(root.querySelectorAll('a')).toHaveLength(5);
    expect(root.querySelector('pre a')).toBeNull();
  });
  it('解析中文编码、空格、平台路径与行号', () => {
    expect(fileTarget('docs/%E8%AF%B4%E6%98%8E%20a.pdf')).toEqual({
      path: 'docs/说明 a.pdf',
    });
    expect(fileTarget('src/main.ts:42:3')).toEqual({
      path: 'src/main.ts',
      line: 42,
    });
    expect(fileTarget('src/main.ts#L9-L12')).toEqual({
      path: 'src/main.ts',
      line: 9,
    });
    expect(fileTarget('file:///C:/work/a%20b.docx')).toEqual({
      path: 'C:/work/a b.docx',
    });
    expect(fileTarget('C:\\work\\说明.txt')).toEqual({
      path: 'C:\\work\\说明.txt',
    });
    expect(codeFileTarget('src/100%25.ts')).toEqual({ path: 'src/100%25.ts' });
  });
  it('Windows 盘符链接经过 Markdown 渲染后仍传递真实路径和行号', () => {
    for (const href of [
      '/C:/work/%E8%AF%B4%E6%98%8E%20a.ts:42:3',
      'C:/work/%E8%AF%B4%E6%98%8E%20a.ts:42:3',
      'file:///C:/work/%E8%AF%B4%E6%98%8E%20a.ts:42:3',
      '/C:/work/%E8%AF%B4%E6%98%8E%20a.ts#L42-L45',
      '/C%3A/work/%E8%AF%B4%E6%98%8E%20a.ts:42',
    ]) {
      const root = document.createElement('div');
      root.innerHTML = markdown(`[文件](${href})`);
      expect(
        root.querySelector('a')?.getAttribute('data-file-path'),
        href,
      ).toBe('C:/work/说明 a.ts');
      expect(root.querySelector('a')?.getAttribute('data-file-line')).toBe(
        '42',
      );
    }
    expect(fileTarget('/tmp/说明.ts:42')).toEqual({
      path: '/tmp/说明.ts',
      line: 42,
    });
    expect(fileTarget('/C:/work/100%2525.ts')).toEqual({
      path: 'C:/work/100%25.ts',
    });
    expect(codeFileTarget('C:\\work\\main.ts:42')).toEqual({
      path: 'C:\\work\\main.ts',
      line: 42,
    });
    expect(fileTarget('/%2Fserver/share.txt')).toBeNull();
    expect(fileTarget('%5C%5Cserver/share.txt')).toBeNull();
  });
  it('代码符号与未知裸后缀不自动识别成文件，行号不能绕过限制', () => {
    for (const value of [
      'FileTarget.line',
      'user.name',
      'process.env',
      'foo.bar',
    ]) {
      for (const suffix of ['', ':42', ':42:3', '#L42', '#L42-L45'])
        expect(codeFileTarget(value + suffix), value + suffix).toBeNull();
    }
  });
  it('常见裸文件名、配置文件与明确路径保留文件入口及行号', () => {
    for (const value of [
      'main.ts',
      'README.md',
      'MAIN.TS',
      'data.jsonl',
      'app.svelte',
      '.gitignore',
      '.gitattributes',
      '.gitmodules',
      '.dockerignore',
      '.editorconfig',
      '.npmrc',
      '.nvmrc',
      '.prettierrc',
      '.eslintrc',
      '.env',
      '.env.local',
      'Dockerfile',
      'Containerfile',
      'Makefile',
      'GNUmakefile',
      'LICENSE',
      'LICENCE',
      'README',
      'CHANGELOG',
      './foo.bar',
      '../foo.bar',
      'src/custom.data',
      './LICENSE',
      '/tmp/说明.data',
      'C:/work/custom.data',
      'C:\\work\\custom.data',
    ]) {
      expect(codeFileTarget(value), value).toEqual({ path: value });
      for (const suffix of [':42', ':42:3', '#L42', '#L42-L45'])
        expect(codeFileTarget(value + suffix), value + suffix).toEqual({
          path: value,
          line: 42,
        });
    }
    expect(codeFileTarget('/C:/work/custom.data:42:3')).toEqual({
      path: 'C:/work/custom.data',
      line: 42,
    });
    expect(codeFileTarget('file:///tmp/custom.data#L42')).toEqual({
      path: '/tmp/custom.data',
      line: 42,
    });
    for (const value of ['.', '..', './', '../', 'src/.', 'src/..', 'src/'])
      expect(codeFileTarget(value), value).toBeNull();
  });
  it('实际 Markdown 渲染保留代码符号文字，仅明确的文件生成入口', () => {
    const root = document.createElement('div');
    root.innerHTML = markdown(
      '继续将受支持的 `:行号[:列号]` 和 `#L行号` 后缀拆成 `FileTarget.line`，不传入文件系统路径。通过渲染测试覆盖 Markdown 规范化后的实际输入。\n\n' +
        '`user.name`、`process.env`、`foo.bar:42`；`main.ts:12`、`README.md`、`./foo.bar#L42`。\n\n' +
        '[`FileTarget.line`](docs/contract.custom:8:2)\n\n```ts\nmain.ts:12\nFileTarget.line\n```',
    );
    const code = [...root.querySelectorAll('p code')];
    for (const value of [
      ':行号[:列号]',
      '#L行号',
      'FileTarget.line',
      'user.name',
      'process.env',
      'foo.bar:42',
    ]) {
      const item = code.find((node) => node.textContent === value);
      expect(item, value).toBeDefined();
      expect(item?.closest('a'), value).toBeNull();
    }
    expect(
      [...root.querySelectorAll('[data-file-path]')].map((node) => [
        node.getAttribute('data-file-path'),
        node.getAttribute('data-file-line'),
      ]),
    ).toEqual([
      ['main.ts', '12'],
      ['README.md', null],
      ['./foo.bar', '42'],
      ['docs/contract.custom', '8'],
    ]);
    expect(root.querySelectorAll('a')).toHaveLength(4);
    expect(root.querySelector('pre a')).toBeNull();
  });
  it('拒绝危险协议、网络共享与歧义代码', () => {
    for (const path of [
      'javascript:alert(1)',
      'data:text/html,x',
      'pi://run',
      'file://server/a',
      '//server/a',
      '\\\\server\\a',
      'a%00.txt',
      'bad%ZZ',
    ])
      expect(fileTarget(path)).toBeNull();
    for (const path of [
      'npm run build',
      'foo.bar()',
      'a = b.ts',
      'hello',
      '<file.ts>',
      'file://server/a',
      '//server/a',
      '\\\\server\\a',
      'pi://run/file.ts',
      'src/file.ts;run',
    ])
      expect(codeFileTarget(path)).toBeNull();
  });
  it('代码样式链接标签保留外层完整目标，不产生嵌套链接', () => {
    const root = document.createElement('div');
    root.innerHTML = markdown('[`main.ts`](src/main.ts#L12)');
    expect(root.querySelectorAll('a')).toHaveLength(1);
    expect(root.querySelector('a')?.getAttribute('data-file-path')).toBe(
      'src/main.ts',
    );
    expect(root.querySelector('a')?.getAttribute('data-file-line')).toBe('12');
  });
  it('清洗后保留文件入口但不允许直接 file 导航或代码块链接', () => {
    const root = document.createElement('div');
    root.innerHTML = markdown(
      '[报告](file:///tmp/%E6%8A%A5%E5%91%8A.pdf)\n\n`src/main.ts:9`\n\n```ts\nsrc/main.ts\n```\n\n[x](javascript:alert(1))\n\n[网页](https://example.com)',
    );
    expect(
      root.querySelector('[data-file-path="/tmp/报告.pdf"]'),
    ).not.toBeNull();
    expect(root.querySelector('[data-file-line="9"]')).not.toBeNull();
    expect(root.querySelector('a[href^="file:"]')).toBeNull();
    expect(root.querySelector('a[href^="javascript:"]')).toBeNull();
    expect(root.querySelector('pre a')).toBeNull();
    expect(root.querySelector('a[href="https://example.com"]')).not.toBeNull();
  });
});
