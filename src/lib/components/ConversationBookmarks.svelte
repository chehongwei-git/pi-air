<!-- 全部摘要后台准备；长目录仅挂载可见行，正文不参与虚拟化。 -->
<script lang="ts">
  import { onDestroy, tick, untrack } from 'svelte';
  import { conversationBookmarks } from '../conversation-bookmarks';
  import { BookmarkDirectory } from '../bookmark-directory.svelte';
  import type { BookmarkPage, Message } from '../types';
  let {
    messages,
    current = '',
    active = true,
    ready = true,
    historyKey = '',
    onhistory,
    onselect,
    oncancel,
  }: {
    messages: Message[];
    current?: string;
    active?: boolean;
    ready?: boolean;
    historyKey?: string;
    onhistory?: (offset: number, revision?: string) => Promise<BookmarkPage>;
    onselect: (id: string) => Promise<void>;
    oncancel: () => void;
  } = $props();
  const directory = new BookmarkDirectory();
  // 只读取 user 的字段，助手正文 token 更新不会重新计算全部目录摘要。
  let local = $derived(conversationBookmarks(messages));
  let items = $derived.by(() => {
    const overrides = new Map(local.map((item) => [item.id, item]));
    const result = directory.items.map((item) => {
      const next = overrides.get(item.id) ?? item;
      overrides.delete(item.id);
      return next;
    });
    return [...result, ...overrides.values()];
  });
  let indexes = $derived(new Map(items.map((item, index) => [item.id, index])));
  let currentIndex = $derived(
    indexes.get(current) ?? Math.max(0, items.length - 1),
  );
  let rail = $derived(
    items.slice(
      Math.max(0, Math.min(items.length - 11, currentIndex - 5)),
      Math.max(0, Math.min(items.length - 11, currentIndex - 5)) + 11,
    ),
  );
  let open = $state(false);
  let prepared = $state(false);
  let selecting = $state(false);
  let selectionError = $state('');
  let slow = $state(false);
  let root = $state<HTMLElement>(null!);
  let list = $state<HTMLDivElement>(null!);
  let trigger = $state<HTMLButtonElement>(null!);
  let revision = 0;
  let savedTop: number | undefined;
  let top = $state(0);
  let focused = 0;
  const rowHeight = 38;
  // 24 行覆盖最大 440px 视口并前后留缓冲，万条提问也不挂载万颗按钮。
  let start = $derived(
    items.length > 100 ? Math.max(0, Math.floor(top / rowHeight) - 5) : 0,
  );
  let end = $derived(
    items.length > 100 ? Math.min(items.length, start + 24) : items.length,
  );
  let visible = $derived(items.slice(start, end));
  let enterTimer: ReturnType<typeof setTimeout>;
  let leaveTimer: ReturnType<typeof setTimeout>;
  function clearTimers(): void {
    clearTimeout(enterTimer);
    clearTimeout(leaveTimer);
  }
  function close(restore = false): void {
    clearTimers();
    revision++;
    if (open && list) savedTop = list.scrollTop;
    // 关闭后立即 inert；先恢复稳定焦点，避免焦点留在不可访问的缓存面板中。
    if (list?.contains(document.activeElement)) {
      if (restore) trigger?.focus({ preventScroll: true });
      else (document.activeElement as HTMLElement)?.blur();
    }
    open = false;
    selecting = false;
    oncancel();
    if (restore) trigger?.focus({ preventScroll: true });
  }
  $effect(() => {
    historyKey;
    untrack(() => {
      close();
      directory.reset();
      prepared = false;
      savedTop = undefined;
      selectionError = '';
    });
  });
  $effect(() => {
    const enabled = active && ready;
    const read = onhistory;
    untrack(() => directory.setActive(enabled, read));
    if (!active) untrack(() => close());
  });
  $effect(() => {
    if (!directory.busy || !open) {
      slow = false;
      return;
    }
    const timer = setTimeout(() => {
      slow = true;
    }, 250);
    return () => clearTimeout(timer);
  });
  onDestroy(() => {
    clearTimers();
    directory.dispose();
    revision++;
    oncancel();
  });
  async function focusIndex(index: number): Promise<void> {
    if (!items.length || !list) return;
    // 窗口化会卸载旧焦点行，先将焦点停在稳定入口，避免 focusout 误判离开目录。
    trigger.focus({ preventScroll: true });
    focused = Math.max(0, Math.min(items.length - 1, index));
    const y = focused * rowHeight + 2;
    if (y < list.scrollTop) list.scrollTop = y;
    else if (y + rowHeight > list.scrollTop + list.clientHeight)
      list.scrollTop = y + rowHeight - list.clientHeight;
    top = list.scrollTop;
    await tick();
    list
      ?.querySelector<HTMLButtonElement>('[data-index="' + focused + '"]')
      ?.focus({ preventScroll: true });
  }
  async function show(keyboard = false): Promise<void> {
    clearTimers();
    if (!active) return;
    if (!open) {
      selectionError = '';
      const request = revision;
      // 首次使用才挂载；未浏览目录的缓存会话不承担额外列表布局与 DOM 成本。
      if (!prepared || !list) {
        top = savedTop ?? Math.max(0, currentIndex * rowHeight - 220);
        prepared = true;
        await tick();
        if (!active || request !== revision || !list) return;
      }
      // 面板保持有界 DOM 与布局，先切换虚拟窗口，再写滚动位置，最后才显现。
      // 不能先写 scrollTop：旧窗口总高度尚未更新时浏览器可能夹紧滚动值。
      const target =
        savedTop ??
        Math.max(0, currentIndex * rowHeight - list.clientHeight / 2);
      top = target;
      await tick();
      if (!active || request !== revision || !list) return;
      list.scrollTop = target;
      top = list.scrollTop;
      open = true;
      if (keyboard) {
        await tick();
        await focusIndex(currentIndex);
      }
    } else if (keyboard) await focusIndex(focused);
  }
  async function select(id: string, keyboard: boolean): Promise<void> {
    if (selecting) return;
    const request = ++revision;
    selecting = true;
    selectionError = '';
    try {
      await onselect(id);
      if (request === revision && active) close(keyboard);
    } catch (cause) {
      if (request === revision) selectionError = String(cause);
    } finally {
      if (request === revision) selecting = false;
    }
  }
  function keydown(event: KeyboardEvent): void {
    if (event.key === 'Escape') {
      event.preventDefault();
      event.stopPropagation();
      close(true);
      return;
    }
    if (!['ArrowUp', 'ArrowDown', 'Home', 'End'].includes(event.key)) return;
    event.preventDefault();
    if (!open) {
      void show(true);
      return;
    }
    const next =
      event.key === 'Home'
        ? 0
        : event.key === 'End'
          ? items.length - 1
          : focused + (event.key === 'ArrowDown' ? 1 : -1);
    void focusIndex(next);
  }
</script>

<svelte:window
  onpointerdown={(event) => {
    if (open && root && !root.contains(event.target as Node)) close();
  }}
  onblur={() => close()}
/>
{#if active && (items.length || onhistory)}
  <!-- svelte-ignore a11y_no_noninteractive_element_interactions (原生导航按钮的冒泡键盘处理。) -->
  <nav
    class="bookmarks"
    class:expanded={open}
    aria-label="提问目录"
    bind:this={root}
    onpointerenter={() => {
      clearTimers();
      // 已展开或正在关闭宽限内不重复计时，避免来回划过产生闪关重开。
      if (!open) enterTimer = setTimeout(() => void show(), 60);
    }}
    onpointerleave={() => {
      clearTimers();
      if (!root.contains(document.activeElement))
        leaveTimer = setTimeout(() => close(), 180);
    }}
    onfocusout={(event) => {
      if (!root.contains(event.relatedTarget as Node)) close();
    }}
    onkeydown={keydown}
  >
    <button
      class="bookmark-trigger"
      bind:this={trigger}
      aria-label="浏览提问目录"
      aria-expanded={open}
      onclick={() => {
        if (open) close(true);
        else void show(true);
      }}
    >
      <span class="bookmark-rail" aria-hidden="true"
        >{#each rail as item (item.id)}<span class:current={item.id === current}
          ></span>{/each}{#if !items.length}<span></span>{/if}</span
      >
    </button>
    {#if prepared}
      <!-- 缓存面板只保留有界行；inert 与 aria-hidden 在关闭时立即生效，不等淡出结束。 -->
      <div class="bookmark-surface" inert={!open} aria-hidden={!open}>
        <div
          class="bookmark-list"
          bind:this={list}
          onscroll={() => {
            top = list.scrollTop;
          }}
        >
          <div style:height={start * rowHeight + 'px'} aria-hidden="true"></div>
          {#each visible as item, index (item.id)}
            <button
              class="bookmark-item"
              class:current={item.id === current}
              data-index={start + index}
              data-bookmark-id={item.id}
              aria-current={item.id === current ? 'location' : undefined}
              aria-label={'定位提问：' + item.label}
              title={item.label}
              disabled={selecting}
              onfocus={() => {
                focused = start + index;
              }}
              onclick={(event) => void select(item.id, event.detail === 0)}
            >
              <span class="bookmark-label">{item.label}</span><span
                class="bookmark-mark"
                aria-hidden="true"
              ></span>
            </button>
          {/each}
          <div
            style:height={(items.length - end) * rowHeight + 'px'}
            aria-hidden="true"
          ></div>
          {#if !items.length && directory.done}<p class="bookmark-empty">
              暂无已发送提问
            </p>{/if}
        </div>
        {#if slow}<div class="bookmark-status" role="status">
            正在整理提问…
          </div>{/if}
        {#if directory.error}<div class="bookmark-status" role="alert">
            {directory.error}<button
              onclick={() => {
                trigger.focus({ preventScroll: true });
                directory.retry();
              }}>重试</button
            >
          </div>{/if}
        {#if selecting}<div class="bookmark-status" role="status">
            正在定位提问…<button onclick={() => close(true)}>取消</button>
          </div>{/if}
        {#if selectionError}<div class="bookmark-status" role="alert">
            {selectionError}
          </div>{/if}
      </div>
    {/if}
  </nav>
{/if}

<style>
  .bookmarks {
    position: absolute;
    z-index: 2;
    inset-inline-end: 12px;
    top: 50%;
    transform: translateY(-50%);
    max-height: calc(100% - 32px);
  }
  .bookmark-trigger {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 32px;
    padding: 8px;
    border: 0;
    background: transparent;
    border-radius: 8px;
  }
  .bookmark-rail {
    display: flex;
    flex-direction: column;
    align-items: flex-end;
    gap: 25px;
  }
  .bookmark-rail > span,
  .bookmark-mark {
    display: block;
    width: 14px;
    height: 3px;
    flex: 0 0 auto;
    border-radius: 3px;
    background: var(--muted);
    opacity: 0.32;
  }
  .bookmark-rail > .current,
  .current .bookmark-mark {
    width: 22px;
    background: var(--accent);
    opacity: 1;
  }
  .expanded .bookmark-trigger {
    opacity: 0;
    pointer-events: none;
  }
  .bookmark-surface {
    position: absolute;
    inset-inline-end: 0;
    top: 50%;
    transform: translate(4px, -50%);
    opacity: 0;
    visibility: hidden;
    pointer-events: none;
    transition:
      opacity 90ms ease-out,
      transform 90ms ease-out,
      visibility 0s 90ms;
    width: min(310px, calc(100cqw - 24px));
    padding: 10px 0;
    border: 1px solid var(--border);
    border-radius: var(--radius-panel);
    background: var(--bg);
    box-shadow: var(--shadow);
  }
  .expanded .bookmark-surface {
    opacity: 1;
    visibility: visible;
    pointer-events: auto;
    transform: translate(0, -50%);
    transition:
      opacity 120ms cubic-bezier(0.2, 0.7, 0.2, 1),
      transform 120ms cubic-bezier(0.2, 0.7, 0.2, 1),
      visibility 0s;
  }
  @media (prefers-reduced-motion: reduce) {
    .bookmark-surface,
    .expanded .bookmark-surface {
      transition: none;
      transform: translate(0, -50%);
    }
  }
  .bookmark-list {
    position: relative;
    max-height: min(440px, calc(100cqh - 92px));
    overflow-y: auto;
    overscroll-behavior: contain;
    scrollbar-width: thin;
    padding: 2px 8px;
  }
  .bookmark-item {
    display: flex;
    align-items: center;
    justify-content: flex-end;
    gap: 16px;
    width: 100%;
    height: 38px;
    border: 0;
    border-radius: 7px;
    padding: 0 10px;
    background: transparent;
    color: var(--muted);
    text-align: end;
    font-size: 13px;
  }
  .bookmark-label {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    min-width: 0;
  }
  .bookmark-mark {
    width: 14px;
  }
  .bookmark-item:hover {
    background: var(--hover);
    color: var(--text);
  }
  .bookmark-item.current {
    color: var(--accent);
    font-weight: 600;
  }
  .bookmark-status {
    padding: 6px 16px;
    font-size: 12px;
    color: var(--muted);
    overflow-wrap: anywhere;
    display: flex;
    gap: 8px;
    align-items: center;
  }
  .bookmark-status button {
    border: 0;
    background: transparent;
    color: var(--accent);
    padding: 4px;
  }
  .bookmark-empty {
    color: var(--muted);
    padding: 8px 16px;
    font-size: 12px;
  }
  button:focus-visible {
    outline: 2px solid var(--accent);
    outline-offset: -2px;
  }
  @container (max-height: 380px) {
    .bookmark-rail {
      gap: 12px;
    }
  }
</style>
