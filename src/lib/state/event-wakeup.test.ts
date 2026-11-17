import { describe, expect, it, vi } from 'vitest';
import { Desktop } from './desktop.svelte';
import { bridge } from '../client';

/** 覆盖空批次返回附近的微任务时序；宿主只唤醒一次也必须消费新消息。 */
describe('event wakeup at batch completion', () => {
  it('isolates retired process responses and resets acknowledgement during queued relocation', async () => {
    let rejectOld!: (error: Error) => void;
    const events = vi.fn(async (runtime: string, _ack: number) => {
      if (runtime === 'old')
        return new Promise<never>((_resolve, reject) => {
          rejectOld = reject;
        });
      return { runtimeId: runtime, sequence: 0, events: [] };
    });
    const desktop = new Desktop({ ...bridge, events });
    desktop.runtimeId = 'old';
    desktop.connection = 'ready';
    desktop.draft = '保留草稿';
    desktop.scrollState = { top: 123, follow: false };
    (desktop as any).ack = 89;
    const pending = (desktop as any).drain();
    (desktop as any).receive(
      { type: 'relocating', runtimeId: 'old', nextRuntimeId: 'relocated-new' },
      0,
    );
    rejectOld(new Error('旧进程已关闭'));
    await pending;
    expect(desktop.error).toBe('');
    expect(desktop.connection).toBe('starting');
    expect(desktop.restoring).toBe(true);
    expect(desktop.draft).toBe('保留草稿');
    expect(desktop.scrollState.top).toBe(123);
    (desktop as any).receive(
      { type: 'available', runtimeId: 'relocated-new' },
      0,
    );
    await (desktop as any).drain();
    expect(events).toHaveBeenCalledWith('relocated-new', 0);
    (desktop as any).receive(
      {
        type: 'failed',
        runtimeId: 'relocated-new',
        error: {
          code: 'PROJECT',
          message: '目录恢复失败',
          diagnosticId: 'test',
        },
      },
      0,
    );
    expect(desktop.restoring).toBe(false);
    expect(desktop.connection).toBe('failed');
    expect(desktop.error).toContain('目录恢复失败');
  });
  it.each([0, 1, 2, 3, 4, 5, 6])(
    'delivers a wakeup delayed by %i microtasks',
    async (delay) => {
      let desktop: Desktop;
      let announced = false;
      let queued = false;
      const events = vi.fn().mockImplementation(async () => {
        if (!announced) {
          announced = true;
          let wake = Promise.resolve();
          for (let index = 0; index < delay; index++)
            wake = wake.then(() => {});
          void wake.then(() => {
            queued = true;
            (desktop as any).receive({ type: 'available', runtimeId: 'r' }, 0);
          });
          return { runtimeId: 'r', sequence: 0, events: [] };
        }
        const messages = queued
          ? [
              {
                type: 'message',
                message: {
                  id: 'arrived',
                  role: 'assistant',
                  text: 'new reply',
                  thinking: '',
                  status: 'done',
                  contentRef: null,
                  name: null,
                },
              },
            ]
          : [];
        queued = false;
        return { runtimeId: 'r', sequence: 1, events: messages };
      });
      desktop = new Desktop({ ...bridge, events });
      desktop.runtimeId = 'r';
      desktop.connection = 'ready';
      await (desktop as any).drain();
      // 不依赖墙钟等待，排空宿主响应及其唤醒链。
      for (let index = 0; index < 30; index++) await Promise.resolve();
      expect(desktop.messages.map((message) => message.id)).toContain(
        'arrived',
      );
      expect(queued).toBe(false);
    },
  );
});
