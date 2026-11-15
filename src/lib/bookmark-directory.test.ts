import { afterEach, expect, it, vi } from 'vitest';
import { BookmarkDirectory } from './bookmark-directory.svelte';
afterEach(() => vi.useRealTimers());

it('串行自动读完全部摘要，悬停/重新启用不重复读取', async () => {
  vi.useFakeTimers();
  const directory = new BookmarkDirectory();
  const read = vi.fn(async (offset: number) => ({
    items: [{ id: String(offset), label: '重复文字' }],
    next: offset + 1,
    done: offset === 3,
    revision: 'a',
  }));
  directory.setActive(true, read);
  await vi.advanceTimersByTimeAsync(200);
  expect(read).toHaveBeenCalledTimes(4);
  expect(directory.items).toHaveLength(4);
  directory.setActive(false, read);
  directory.setActive(true, read);
  await vi.advanceTimersByTimeAsync(300);
  expect(read).toHaveBeenCalledTimes(4);
  directory.dispose();
});
it('隐藏时停止后续页，在途结果缓存且恢复后从游标继续，最多一请求', async () => {
  vi.useFakeTimers();
  const directory = new BookmarkDirectory();
  let release!: (page: any) => void;
  const read = vi.fn(
    () =>
      new Promise<any>((resolve) => {
        release = resolve;
      }),
  );
  directory.setActive(true, read);
  await vi.advanceTimersByTimeAsync(50);
  directory.setActive(true, read);
  await vi.advanceTimersByTimeAsync(300);
  expect(read).toHaveBeenCalledTimes(1);
  directory.setActive(false, read);
  release({
    items: [{ id: 'first', label: 'a' }],
    next: 1,
    done: false,
    revision: 'a',
  });
  await vi.advanceTimersByTimeAsync(300);
  expect(directory.items).toHaveLength(1);
  expect(read).toHaveBeenCalledTimes(1);
  directory.setActive(true, read);
  await vi.advanceTimersByTimeAsync(50);
  expect(read).toHaveBeenLastCalledWith(1, 'a');
  directory.dispose();
});
it('分支失效丢弃迟到页、失败不循环重试，显式重试可恢复', async () => {
  vi.useFakeTimers();
  const directory = new BookmarkDirectory();
  let release!: (page: any) => void;
  const read = vi.fn(
    () =>
      new Promise<any>((resolve) => {
        release = resolve;
      }),
  );
  directory.setActive(true, read);
  await vi.advanceTimersByTimeAsync(50);
  directory.reset();
  directory.setActive(false, read);
  release({
    items: [{ id: 'stale', label: '旧目录' }],
    next: 1,
    done: true,
    revision: 'a',
  });
  await vi.advanceTimersByTimeAsync(300);
  expect(directory.items).toEqual([]);
  const broken = vi.fn(async () => ({
    items: [],
    next: 0,
    done: false,
    revision: 'b',
  }));
  directory.setActive(true, broken);
  await vi.advanceTimersByTimeAsync(1000);
  expect(broken).toHaveBeenCalledTimes(1);
  expect(directory.error).toContain('游标');
  const good = vi.fn(async () => ({
    items: [{ id: 'ok', label: '恢复' }],
    next: 1,
    done: true,
    revision: 'c',
  }));
  directory.setActive(true, good);
  directory.retry();
  await vi.advanceTimersByTimeAsync(100);
  expect(directory.done).toBe(true);
  expect(directory.items[0].id).toBe('ok');
  directory.dispose();
});
it('一万条摘要仅需 40 个有界请求，完成后不轮询', async () => {
  vi.useFakeTimers();
  const directory = new BookmarkDirectory();
  const read = vi.fn(async (offset: number) => {
    const next = Math.min(10000, offset + 256);
    return {
      items: Array.from({ length: next - offset }, (_, index) => ({
        id: String(offset + index),
        label: '提问摘要',
      })),
      next,
      done: next === 10000,
      revision: 'a',
    };
  });
  directory.setActive(true, read);
  await vi.advanceTimersByTimeAsync(5000);
  expect(directory.items).toHaveLength(10000);
  expect(read).toHaveBeenCalledTimes(40);
  await vi.advanceTimersByTimeAsync(5000);
  expect(read).toHaveBeenCalledTimes(40);
  directory.dispose();
});
