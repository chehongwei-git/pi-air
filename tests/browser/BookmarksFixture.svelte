<!-- 目录分页独立于正文：可模拟慢请求、失败和会话切换。 -->
<script lang="ts">
  import Transcript from '../../src/lib/components/Transcript.svelte';
  import type { Message } from '../../src/lib/types';
  import { conversationBookmarks } from '../../src/lib/conversation-bookmarks';
  const host = window as any;
  const all: Message[] = Array.from({ length: 80 }, (_, index) => ({
    id: `bookmark-${index}`,
    role: index % 2 ? 'assistant' : 'user',
    text:
      index % 2
        ? '回答内容。'.repeat(100)
        : `第 ${index / 2 + 1} 次提问：请解释这段代码`,
    thinking: '',
    contentRef: null,
    status: 'done',
    name: null,
  }));
  let start = $state(40);
  let messages = $state(all.slice(start));
  let key = $state('session-a');
  let active = $state(true);
  host.historyCalls = 0;
  host.bodyCalls = 0;
  host.failHistory = new URLSearchParams(location.search).has('fail');
  host.delay = false;
  host.release = undefined;
  host.switchSession = () => {
    key = 'session-b';
    messages = [{ ...all[0], id: 'other', text: '另一个会话' }];
    start = 0;
  };
  host.setActive = (value: boolean) => {
    active = value;
  };
  host.append = () => {
    messages = [
      ...messages,
      {
        ...all[1],
        id: `stream-${messages.length}`,
        text: '新增流式输出'.repeat(200),
      },
    ];
  };
  host.catalog = new URLSearchParams(location.search).has('large')
    ? Array.from({ length: 10000 }, (_, index) => ({
        id: `large-${index}`,
        label: `大目录提问 ${index}`,
      }))
    : undefined;
  async function history(offset: number, revision?: string) {
    host.historyCalls++;
    if (host.delay)
      await new Promise<void>((resolve) => {
        host.release = resolve;
      });
    if (host.failHistory) throw new Error('模拟目录读取失败');
    const catalog =
      host.catalog ??
      conversationBookmarks(key === 'session-b' ? messages : all);
    const next = Math.min(catalog.length, offset + 256);
    return {
      items: catalog.slice(offset, next),
      next,
      done: next === catalog.length,
      revision: revision ?? key,
    };
  }
  async function older(beforeInsert: () => void) {
    host.bodyCalls++;
    if (host.delay)
      await new Promise<void>((resolve) => {
        host.release = resolve;
      });
    beforeInsert();
    start = Math.max(0, start - 20);
    messages = all.slice(start);
  }
</script>

<div class="fixture">
  <Transcript
    {messages}
    {active}
    historyKey={key}
    bookmarkHistory={history}
    hasMore={start > 0}
    onolder={older}
    onfull={() => {}}
    onload={async () => {}}
    onvisible={() => {}}
    onerror={() => {}}
    toolViews={{}}
    ontoggletool={() => {}}
    read={async () => ({ text: '', next: 0, done: true })}
  />
  <textarea aria-label="草稿" placeholder="草稿保持不变"></textarea>
</div>

<style>
  .fixture {
    display: flex;
    flex-direction: column;
    height: 100%;
    width: 100%;
  }
  textarea {
    margin: 16px 48px;
    min-height: 72px;
    border: 1px solid var(--border);
    background: var(--input-surface);
    color: var(--text);
    border-radius: 12px;
    padding: 12px;
  }
</style>
