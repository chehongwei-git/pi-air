/** 冷启动模型目录读取与异步隔离，不启动 Pi 或访问认证文件。 */
import { describe, expect, it, vi } from 'vitest';
import { Desktop } from './desktop.svelte';
import { bridge } from '../client';

const models = [
  { provider: 'test', id: 'a', name: 'A' },
  { provider: 'test', id: 'b', name: 'B' },
];
function fixture() {
  const action = vi.fn().mockImplementation(async (_runtime, command) => {
    if (command.type === 'models') return { models };
    return {};
  });
  const desktop = new Desktop({ ...bridge, action });
  desktop.runtimeId = 'startup';
  desktop.connection = 'ready';
  desktop.snapshot = { model: models[0], thinkingLevel: 'high' };
  desktop.draft = '未发送草稿';
  return { desktop, action };
}
describe('模型目录初始化', () => {
  it('首次读取即可切换其他模型，不发送消息且保留草稿和强度', async () => {
    const { desktop, action } = fixture();
    await desktop.refreshMetadata();
    expect(desktop.models).toEqual(models);
    expect(desktop.modelsLoading).toBe(false);
    expect(desktop.modelsError).toBe('');
    expect(desktop.draft).toBe('未发送草稿');
    expect(desktop.selectedThinking).toBe('high');
    expect(action.mock.calls.map(([, command]) => command.type)).toEqual([
      'models',
      'thinking_levels',
      'commands',
    ]);
  });
  it('慢加载合并并发读取，失败保留当前模型并可显式重试', async () => {
    const { desktop, action } = fixture();
    let reject!: (error: Error) => void;
    action.mockImplementationOnce(
      () =>
        new Promise((_, fail) => {
          reject = fail;
        }),
    );
    const first = desktop.refreshMetadata();
    expect(desktop.refreshMetadata()).toBe(first);
    expect(desktop.modelsLoading).toBe(true);
    reject(new Error('认证目录不可用'));
    await first;
    expect(desktop.modelsError).toBe('认证目录不可用');
    expect(desktop.selectedModel).toEqual(models[0]);
    await desktop.refreshModels();
    expect(desktop.modelsError).toBe('');
    expect(desktop.models).toEqual(models);
  });
  it.each(['runtime', 'config'])('忽略 %s 变化前的迟到目录', async (change) => {
    const { desktop, action } = fixture();
    let release!: (value: unknown) => void;
    action.mockImplementationOnce(
      () =>
        new Promise((resolve) => {
          release = resolve;
        }),
    );
    const old = desktop.refreshMetadata();
    if (change === 'runtime') desktop.runtimeId = 'new-runtime';
    else desktop.modelConfigSaved({ providers: [] } as never);
    await desktop.refreshMetadata();
    release({ models: [{ provider: 'stale', id: 'old' }] });
    await old;
    expect(desktop.models).toEqual(models);
    expect(desktop.modelsLoading).toBe(false);
  });
});
