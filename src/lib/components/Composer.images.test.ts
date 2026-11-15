/** 三种附件入口共享真实文件读取；摘要用稳定边界替身。 */
import '@testing-library/jest-dom/vitest';
import { cleanup, fireEvent, render, waitFor } from '@testing-library/svelte';
import { afterEach, describe, expect, it, vi } from 'vitest';
import Composer from './Composer.svelte';
vi.mock('../file-drop', () => ({
  registerFileDrop: (target: { drop: (id: string) => void }) => {
    native.drop = target.drop;
    return () => {
      native.drop = undefined;
    };
  },
}));
const randomUUID = crypto.randomUUID.bind(crypto);
const png = new Uint8Array([137, 80, 78, 71, 13, 10, 26, 10]);
const native = vi.hoisted(() => ({
  enabled: false,
  invoke: vi.fn(),
  drop: undefined as ((id: string) => void) | undefined,
}));
vi.mock('@tauri-apps/api/core', async (original) => ({
  ...(await original<object>()),
  isTauri: () => native.enabled,
  invoke: native.invoke,
}));
afterEach(() => {
  cleanup();
  native.enabled = false;
  native.invoke.mockReset();
  vi.unstubAllGlobals();
});
describe('image composer', () => {
  it.each([false, true])(
    'pastes native files once even when WebView exposes files=%s',
    async (exposed) => {
      native.enabled = true;
      native.invoke.mockResolvedValue([
        {
          id: 'key',
          kind: 'path',
          name: 'prod.keys',
          path: '/tmp/prod.keys',
          directory: false,
        },
      ]);
      const onsend = vi.fn();
      const view = render(Composer, { value: '草稿', onsend, onstop: vi.fn() });
      await fireEvent.paste(view.getByRole('textbox'), {
        clipboardData: {
          files: exposed ? [new File(['secret'], 'prod.keys')] : [],
          getData: () => '/tmp/prod.keys',
        },
      });
      await waitFor(() =>
        expect(
          view.getAllByRole('button', { name: /查看文件引用/ }),
        ).toHaveLength(1),
      );
      expect(native.invoke).toHaveBeenCalledWith('paste_files', {
        budget: { count: 0, bytes: 0 },
      });
      expect(view.getByRole('textbox')).toHaveValue('草稿');
      expect(view.queryByRole('alert')).not.toBeInTheDocument();
      expect(onsend).not.toHaveBeenCalled();
      await fireEvent.click(
        view.getByRole('button', { name: '移除prod.keys' }),
      );
      expect(
        view.queryByRole('button', { name: /查看文件引用/ }),
      ).not.toBeInTheDocument();
    },
  );
  it.each([false, true])(
    'retains text insertion and undo when native lookup fails=%s',
    async (failed) => {
      native.enabled = true;
      if (failed)
        native.invoke.mockRejectedValue(new Error('clipboard unavailable'));
      else native.invoke.mockResolvedValue([]);
      const view = render(Composer, {
        value: '前后',
        onsend: vi.fn(),
        onstop: vi.fn(),
      });
      const input = view.getByRole('textbox') as HTMLTextAreaElement;
      input.setSelectionRange(1, 1);
      await fireEvent.paste(input, {
        clipboardData: { files: [], getData: () => '粘贴' },
      });
      await waitFor(() => expect(input).toHaveValue('前粘贴后'));
      await fireEvent.keyDown(input, { key: 'z', ctrlKey: true });
      await waitFor(() => expect(input).toHaveValue('前后'));
      expect(view.queryByRole('alert')).not.toBeInTheDocument();
    },
  );
  it('blocks sending while reading and discards clipboard results on context change', async () => {
    native.enabled = true;
    let resolve!: (items: unknown[]) => void;
    native.invoke.mockImplementation(
      () =>
        new Promise((done) => {
          resolve = done;
        }),
    );
    const onsend = vi.fn();
    const view = render(Composer, {
      value: '草稿',
      historyKey: 'first',
      onsend,
      onstop: vi.fn(),
    });
    await fireEvent.paste(view.getByRole('textbox'), {
      clipboardData: { files: [], getData: () => '旧文本' },
    });
    expect(view.getByRole('button', { name: '发送消息' })).toBeDisabled();
    await fireEvent.keyDown(view.getByRole('textbox'), { key: 'Enter' });
    expect(onsend).not.toHaveBeenCalled();
    await view.rerender({ historyKey: 'second', value: '新草稿' });
    resolve([]);
    await waitFor(() =>
      expect(view.getByRole('button', { name: '发送消息' })).toBeEnabled(),
    );
    expect(view.getByRole('textbox')).toHaveValue('新草稿');
  });
  it('shows native file-read errors inline without losing the draft', async () => {
    native.enabled = true;
    native.invoke.mockRejectedValue(new Error('无法访问系统剪贴板'));
    const view = render(Composer, {
      value: '草稿',
      onsend: vi.fn(),
      onstop: vi.fn(),
    });
    await fireEvent.paste(view.getByRole('textbox'), {
      clipboardData: {
        files: [new File(['key'], 'prod.keys')],
        getData: () => '',
      },
    });
    await waitFor(() =>
      expect(view.getByRole('alert')).toHaveTextContent('无法访问系统剪贴板'),
    );
    expect(view.getByRole('textbox')).toHaveValue('草稿');
  });
  it('uses the native picker, prevents duplicate selection and ignores late results', async () => {
    native.enabled = true;
    let resolve!: (images: unknown[]) => void;
    native.invoke.mockImplementation(
      () =>
        new Promise((done) => {
          resolve = done;
        }),
    );
    const view = render(Composer, {
      historyKey: 'first',
      onsend: vi.fn(),
      onstop: vi.fn(),
    });
    const button = view.getByRole('button', { name: '添加文件' });
    const htmlPicker = vi.spyOn(view.getByLabelText('选择文件'), 'click');
    await fireEvent.click(button);
    expect(native.invoke).toHaveBeenCalledWith('pick_files', {
      budget: { count: 0, bytes: 0 },
    });
    expect(button).toBeDisabled();
    expect(htmlPicker).not.toHaveBeenCalled();
    await view.rerender({ historyKey: 'second' });
    resolve([
      { id: 'old', name: 'old.png', mimeType: 'image/png', data: 'aA==' },
    ]);
    await waitFor(() => expect(button).toBeEnabled());
    expect(view.queryByAltText('old.png')).not.toBeInTheDocument();
  });
  it('preserves text on native cancellation and reports read failures inline', async () => {
    native.enabled = true;
    native.invoke
      .mockResolvedValueOnce([])
      .mockRejectedValueOnce(new Error('图片读取失败'));
    const view = render(Composer, {
      value: '保留草稿',
      onsend: vi.fn(),
      onstop: vi.fn(),
    });
    const button = view.getByRole('button', { name: '添加文件' });
    await fireEvent.click(button);
    await waitFor(() => expect(button).toBeEnabled());
    expect(view.queryByRole('alert')).not.toBeInTheDocument();
    await fireEvent.click(button);
    await waitFor(() =>
      expect(view.getByRole('alert')).toHaveTextContent('图片读取失败'),
    );
    expect(view.getByRole('textbox')).toHaveValue('保留草稿');
  });
  it.each(['choose', 'paste', 'drop'])(
    'adds and removes an image through %s and allows an image-only send',
    async (method) => {
      vi.stubGlobal('crypto', {
        randomUUID,
        subtle: { digest: async () => new Uint8Array([1, 2]).buffer },
      });
      const onsend = vi.fn();
      const view = render(Composer, { onsend, onstop: vi.fn() });
      const imageButton = view.getByRole('button', { name: '添加文件' });
      expect(imageButton.textContent?.trim()).toBe('');
      expect(imageButton.querySelector('svg')).toBeInTheDocument();
      const picker = view.getByLabelText('选择文件');
      expect(picker).not.toHaveAttribute('accept');
      const choose = vi.spyOn(picker, 'click');
      await fireEvent.click(imageButton);
      expect(choose).toHaveBeenCalledOnce();
      const file = new File([png], '截图.png', { type: 'image/png' });
      if (method === 'choose')
        await fireEvent.change(view.getByLabelText('选择文件'), {
          target: { files: [file] },
        });
      if (method === 'paste')
        await fireEvent.paste(view.getByRole('textbox'), {
          clipboardData: { files: [file] },
        });
      if (method === 'drop')
        await fireEvent.drop(view.getByRole('group', { name: '消息编辑器' }), {
          dataTransfer: { files: [file] },
        });
      await waitFor(() =>
        expect(view.getByAltText('截图.png')).toBeInTheDocument(),
      );
      await fireEvent.keyDown(view.getByRole('textbox'), { key: 'Enter' });
      expect(onsend).toHaveBeenCalledOnce();
      await fireEvent.click(view.getByRole('button', { name: '移除截图.png' }));
      expect(view.getByRole('button', { name: '发送消息' })).toBeDisabled();
    },
  );
  it('ignores a completed image read after switching context', async () => {
    let resolve!: (value: ArrayBuffer) => void;
    vi.stubGlobal('crypto', {
      randomUUID,
      subtle: {
        digest: () =>
          new Promise<ArrayBuffer>((done) => {
            resolve = done;
          }),
      },
    });
    const view = render(Composer, {
      historyKey: 'first',
      onsend: vi.fn(),
      onstop: vi.fn(),
    });
    await fireEvent.change(view.getByLabelText('选择文件'), {
      target: { files: [new File([png], 'old.png', { type: 'image/png' })] },
    });
    await waitFor(() => expect(resolve).toBeDefined());
    await view.rerender({ historyKey: 'second' });
    resolve(new Uint8Array([1]).buffer);
    await waitFor(() =>
      expect(view.queryByRole('status')).not.toBeInTheDocument(),
    );
    expect(view.queryByAltText('old.png')).not.toBeInTheDocument();
  });
  it('adds mixed native files in source order and reports individual errors without losing successes', async () => {
    native.enabled = true;
    native.invoke.mockResolvedValue([
      {
        id: 'file',
        kind: 'path',
        name: '需求.pdf',
        path: '/tmp/需求.pdf',
        directory: false,
      },
      {
        id: 'image',
        kind: 'image',
        image: {
          id: 'hash',
          name: '截图.png',
          mimeType: 'image/png',
          data: 'iVBORw0KGgo=',
        },
      },
      { kind: 'error', name: '坏图.png', message: '图片格式无效' },
      {
        id: 'svg',
        kind: 'path',
        name: '图.svg',
        path: '/tmp/图.svg',
        directory: false,
        notice: '此格式按路径引用',
      },
    ]);
    const onsend = vi.fn();
    const view = render(Composer, { onsend, onstop: vi.fn() });
    await fireEvent.click(view.getByRole('button', { name: '添加文件' }));
    await waitFor(() =>
      expect(view.getByRole('button', { name: '发送消息' })).toBeEnabled(),
    );
    const cards = view.container.querySelectorAll('[data-attachment]');
    expect(
      [...cards].map((card) => (card as HTMLElement).dataset.attachment),
    ).toEqual(['file', 'image', 'svg']);
    expect(view.getByRole('alert')).toHaveTextContent('坏图.png：图片格式无效');
    expect(view.getByText('此格式按路径引用')).toBeVisible();
    await fireEvent.keyDown(view.getByRole('textbox'), { key: 'Enter' });
    expect(onsend).toHaveBeenCalledOnce();
  });
  it('consumes native drop tickets immediately but commits batches in order and discards inactive results', async () => {
    native.enabled = true;
    const resolve: ((items: unknown[]) => void)[] = [];
    native.invoke.mockImplementation(
      () => new Promise((done) => resolve.push(done)),
    );
    const view = render(Composer, { onsend: vi.fn(), onstop: vi.fn() });
    native.drop!('first');
    native.drop!('second');
    expect(native.invoke.mock.calls.map((call) => call[1].dropId)).toEqual([
      'first',
      'second',
    ]);
    const file = (id: string) => ({
      id,
      kind: 'path' as const,
      name: `${id}.txt`,
      path: `/tmp/${id}.txt`,
      directory: false,
    });
    resolve[1]([file('second')]);
    expect(
      view.queryByRole('button', { name: /查看文件引用/ }),
    ).not.toBeInTheDocument();
    resolve[0]([file('first')]);
    await waitFor(() =>
      expect(
        view.getAllByRole('button', { name: /查看文件引用/ }),
      ).toHaveLength(2),
    );
    expect(
      [
        ...view.container.querySelectorAll<HTMLElement>('[data-attachment]'),
      ].map((item) => item.dataset.attachment),
    ).toEqual(['first', 'second']);
    native.drop!('late');
    // 宿主通过双向绑定持有已接受附件；重设测试 props 时显式模拟这个外部草稿。
    await view.rerender({
      active: false,
      attachments: [file('first'), file('second')],
    });
    resolve[2]([file('late')]);
    await waitFor(() =>
      expect(view.getByRole('button', { name: '添加文件' })).toBeDisabled(),
    );
    expect(view.queryByText('late')).not.toBeInTheDocument();
    expect(view.getAllByRole('button', { name: /查看文件引用/ })).toHaveLength(
      2,
    );
  });
  it('accepts an empty-MIME image by signature while retaining errors for the other browser files', async () => {
    vi.stubGlobal('crypto', {
      randomUUID,
      subtle: { digest: async () => new Uint8Array([3]).buffer },
    });
    const view = render(Composer, { onsend: vi.fn(), onstop: vi.fn() });
    await fireEvent.change(view.getByLabelText('选择文件'), {
      target: {
        files: [
          new File(['pdf'], 'guide.pdf', { type: 'application/pdf' }),
          new File([png], 'valid.PNG'),
          new File(['broken'], 'bad.png', { type: 'image/png' }),
        ],
      },
    });
    await waitFor(() => expect(view.getByAltText('valid.PNG')).toBeVisible());
    await waitFor(() =>
      expect(view.getByRole('alert')).toHaveTextContent(
        'bad.png：图片内容与格式不符',
      ),
    );
    expect(view.getByRole('alert')).toHaveTextContent('guide.pdf');
    expect(view.queryByAltText('bad.png')).not.toBeInTheDocument();
    expect(view.getByRole('button', { name: '发送消息' })).toBeEnabled();
  });
  it('reports missing browser paths inline without fabricating a reference', async () => {
    const onsend = vi.fn();
    const view = render(Composer, { onsend, onstop: vi.fn() });
    await fireEvent.change(view.getByLabelText('选择文件'), {
      target: {
        files: [new File(['svg'], 'test.svg', { type: 'image/svg+xml' })],
      },
    });
    await waitFor(() =>
      expect(view.getByRole('alert')).toHaveTextContent(
        '无法取得此文件的完整路径',
      ),
    );
    expect(onsend).not.toHaveBeenCalled();
  });
});
