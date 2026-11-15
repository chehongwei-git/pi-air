import type { BookmarkPage } from './types';
import type { ConversationBookmark } from './conversation-bookmarks';

/** 会话视图拥有的摘要缓存；串行分批读完，隐藏时停止后续批次，不依赖悬停。 */
export class BookmarkDirectory {
  items = $state<ConversationBookmark[]>([]);
  busy = $state(false);
  done = $state(false);
  error = $state('');
  private offset = 0;
  private revision: string | undefined;
  private generation = 0;
  private timer: ReturnType<typeof setTimeout> | undefined;
  private enabled = false;
  private ids = new Map<string, number>();
  private read:
    ((offset: number, revision?: string) => Promise<BookmarkPage>) | undefined;

  reset(): void {
    this.generation++;
    clearTimeout(this.timer);
    this.timer = undefined;
    this.items = [];
    this.ids.clear();
    this.offset = 0;
    this.revision = undefined;
    this.busy = false;
    this.done = false;
    this.error = '';
    this.schedule();
  }
  setActive(active: boolean, read: typeof this.read): void {
    this.enabled = active;
    this.read = read;
    if (!active) {
      clearTimeout(this.timer);
      this.timer = undefined;
    } else this.schedule();
  }
  retry(): void {
    // 失败重读保留已展示摘要；新修订按 ID 归并，成功完整读完前不闪空目录。
    this.generation++;
    this.offset = 0;
    this.revision = undefined;
    this.busy = false;
    this.done = false;
    this.error = '';
    this.schedule();
  }
  private schedule(): void {
    if (
      !this.enabled ||
      !this.read ||
      this.busy ||
      this.done ||
      this.error ||
      this.timer !== undefined
    )
      return;
    // 先让首屏与输入就绪，每批让出事件循环；无轮询、无并行请求。
    this.timer = setTimeout(() => {
      this.timer = undefined;
      void this.load();
    }, 40);
  }
  private async load(): Promise<void> {
    if (!this.enabled || !this.read || this.busy) return;
    const generation = this.generation;
    this.busy = true;
    try {
      const page = await this.read(this.offset, this.revision);
      if (generation !== this.generation) return;
      if (
        (this.revision !== undefined && page.revision !== this.revision) ||
        page.next < this.offset ||
        (!page.done && page.next <= this.offset)
      )
        throw new Error('目录历史或游标已变化，请重试');
      // 摘要按 ID 原位归并，保留重复文字；每批只发布一次响应式更新。
      for (const item of page.items) {
        const index = this.ids.get(item.id);
        if (index === undefined) {
          this.ids.set(item.id, this.items.length);
          this.items.push(item);
        } else this.items[index] = item;
      }
      this.offset = page.next;
      this.revision = page.revision;
      this.done = page.done;
    } catch (cause) {
      if (generation === this.generation) this.error = String(cause);
    } finally {
      if (generation === this.generation) {
        this.busy = false;
        this.schedule();
      }
    }
  }
  dispose(): void {
    this.enabled = false;
    this.generation++;
    clearTimeout(this.timer);
  }
}
