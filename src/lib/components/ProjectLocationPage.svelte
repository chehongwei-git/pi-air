<!-- 优先后台恢复；只有无法可靠识别的目录才需要此内嵌页面，不中断会话 DOM。 -->
<script lang="ts">
  import { onMount, onDestroy, tick } from 'svelte';
  import type { Workspace } from '../state/workspace.svelte';
  import type { DirectoryListing, ProjectMergePreview } from '../types';
  import { errorMessage } from '../client';
  import Icon from './Icon.svelte';
  let {
    workspace,
    projectId,
    onclose,
  }: { workspace: Workspace; projectId: string; onclose: () => void } =
    $props();
  const project = $derived(
    workspace.active.settings.projects?.find(
      (p) => p.id === projectId || p.mergedIds?.includes(projectId),
    ),
  );
  const waiting = $derived(!!project?.pendingPath);
  const location = $derived(
    workspace.projectLocations?.find((p) => p.id === project?.id),
  );
  let path = $state('');
  let listing = $state<DirectoryListing>();
  let preview = $state<ProjectMergePreview>();
  let checking = $state(false);
  let busy = $state(false);
  let message = $state('');
  let details = $state('');
  let success = $state('');
  let input = $state<HTMLInputElement>();
  let action = $state<HTMLButtonElement>();
  let back = $state<HTMLButtonElement>();
  let timer: ReturnType<typeof setTimeout> | undefined;
  let generation = 0;
  let composing = false;
  let disposed = false;
  onMount(() => {
    path = project?.pendingPath ?? project?.currentPath ?? '';
    input?.focus();
  });
  onDestroy(() => {
    disposed = true;
    generation++;
    clearTimeout(timer);
  });

  /** 常见状态只给可执行的短说明；诊断与系统消息始终留在折叠详情。 */
  function fail(cause: unknown, saving = false) {
    const code =
      typeof cause === 'object' && cause && 'code' in cause
        ? String(cause.code)
        : '';
    message =
      code === 'PROJECT_NOT_FOUND'
        ? '找不到这个目录'
        : code === 'PROJECT_PERMISSION'
          ? '无法访问这个目录'
          : code === 'PROJECT_REVISION'
            ? '目录信息已变化，请重新检查'
            : code === 'PROJECT_AMBIGUOUS'
              ? '目录归属不明确，请更换目录或检查项目记录'
              : code === 'PROJECT_PENDING'
                ? '正在完成目录更新，请稍候'
                : saving
                  ? '没有保存成功，输入已保留'
                  : '暂时无法检查这个目录，请重试';
    details = errorMessage(cause);
  }
  async function check() {
    clearTimeout(timer);
    const request = ++generation;
    preview = undefined;
    message = '';
    details = '';
    success = '';
    if (!path.trim() || !project) {
      checking = false;
      return;
    }
    const target = path.trim();
    checking = true;
    try {
      const result = await workspace.previewProjectMerge(project.id, target);
      if (!disposed && request === generation) preview = result;
    } catch (cause) {
      if (!disposed && request === generation) fail(cause);
    } finally {
      if (!disposed && request === generation) checking = false;
    }
  }
  function changed() {
    generation++;
    clearTimeout(timer);
    preview = undefined;
    message = '';
    details = '';
    success = '';
    checking = !!path.trim() && !composing;
    if (!composing) timer = setTimeout(() => void check(), 350);
  }
  async function browse(target: string) {
    const request = ++generation;
    clearTimeout(timer);
    checking = true;
    preview = undefined;
    message = '';
    details = '';
    success = '';
    try {
      const result = await workspace.browseDirectories(target);
      if (!disposed && request === generation) listing = result;
    } catch (cause) {
      if (!disposed && request === generation) fail(cause);
    } finally {
      if (!disposed && request === generation) checking = false;
    }
  }
  async function openBrowser() {
    if (listing) {
      listing = undefined;
      input?.focus();
      return;
    }
    // 旧目录常已不存在；从其父目录开始浏览，不把浏览行为当作关联确认。
    const separator = Math.max(path.lastIndexOf('/'), path.lastIndexOf('\\'));
    const parent =
      separator === 0
        ? path.slice(0, 1)
        : separator === 2 && path[1] === ':'
          ? path.slice(0, 3)
          : path.slice(0, separator);
    await browse(parent || path);
  }
  function selectDirectory(target: string) {
    path = target;
    listing = undefined;
    void check();
    input?.focus();
  }
  async function save() {
    if (!preview || busy || checking) return;
    const selected = preview;
    busy = true;
    message = '';
    details = '';
    try {
      await workspace.mergeProjects(selected);
      if (disposed) return;
      path = selected.target;
      preview = undefined;
      success = project?.pendingPath
        ? ''
        : selected.targetId
          ? '项目已合并。'
          : '目录已更新。';
    } catch (cause) {
      if (!disposed) fail(cause, true);
    } finally {
      busy = false;
      await tick();
      if (!disposed) {
        if (message) action?.focus();
        else back?.focus();
      }
    }
  }
  async function retryWaiting() {
    busy = true;
    message = '';
    details = '';
    try {
      await workspace.refreshProjectLocations();
    } catch (cause) {
      fail(cause, true);
    } finally {
      busy = false;
    }
  }
  async function cancel() {
    if (!project || busy) return;
    busy = true;
    message = '';
    details = '';
    try {
      await workspace.cancelProjectMerge(project.id);
      success = '已取消等待，可重新选择目录。';
    } catch (cause) {
      fail(cause, true);
    } finally {
      busy = false;
      await tick();
      if (!disposed) input?.focus();
    }
  }
</script>

<section class="project-location-page" aria-label="项目目录关联">
  <div class="location-content">
    <header>
      <button class="text-button" bind:this={back} onclick={onclose}
        >返回会话</button
      >
      <h1>项目目录</h1>
      <p>
        {project
          ? workspace.active.projectName(project.currentPath)
          : '项目记录不可用'}
      </p>
    </header>
    {#if project}
      <div class="location-current">
        <span>原位置</span><code>{project.currentPath}</code>
      </div>
      {#if waiting}
        <div class="location-status" role="status">
          <p>
            {location?.state === 'unavailable'
              ? '目录更新未完成，请检查目标后重试'
              : project.pendingMerge?.retiring
                ? '正在完成目录更新…'
                : project.pendingMerge?.targetId
                  ? '当前任务结束后自动合并'
                  : '当前任务结束后自动更新目录'}
          </p>
          <code>{project.pendingPath}</code>
          {#if location?.error}<details>
              <summary>查看详情</summary>
              <pre>{errorMessage(location.error)}</pre>
            </details>{/if}
          {#if location?.state === 'unavailable'}<button
              class="text-button"
              disabled={busy}
              onclick={() => void retryWaiting()}>重新检查</button
            >{/if}
          {#if !project.pendingMerge?.retiring}<button
              class="text-button"
              disabled={busy}
              onclick={() => void cancel()}>取消等待</button
            >{/if}
        </div>
      {:else}
        <form
          onsubmit={(event) => {
            event.preventDefault();
            if (!composing) void check();
          }}
        >
          <label for="project-directory-path">重新关联目录</label>
          <p class="help">选择改名或移动后的文件夹，继续使用原有会话和草稿。</p>
          <div class="path-controls">
            <input
              id="project-directory-path"
              bind:this={input}
              bind:value={path}
              oninput={changed}
              oncompositionstart={() => {
                composing = true;
                generation++;
                clearTimeout(timer);
                preview = undefined;
                checking = false;
              }}
              oncompositionend={() => {
                composing = false;
                changed();
              }}
              disabled={busy}
              autocomplete="off"
              spellcheck="false"
            />
            <button
              type="button"
              disabled={busy || checking || !path.trim()}
              onclick={() => void openBrowser()}>浏览</button
            >
          </div>
        </form>
        {#if listing}
          <nav class="directory-browser" aria-label="文件夹浏览">
            <div class="directory-heading">
              <code>{listing.path}</code>
              {#if listing.parent}<button
                  class="text-button"
                  disabled={checking}
                  onclick={() => void browse(listing!.parent!)}>上一级</button
                >{/if}
            </div>
            <ul>
              {#each listing.directories as directory (directory)}
                <li>
                  <button
                    class="text-button"
                    disabled={checking}
                    onclick={() => selectDirectory(directory)}
                    ><Icon name="folder" size={16} /><span
                      >{directory.split(/[\\/]/).at(-1)}</span
                    ></button
                  >
                </li>
              {:else}<li class="empty">没有子文件夹</li>{/each}
            </ul>
            <button
              class="text-button"
              onclick={() => selectDirectory(listing!.path)}>使用此目录</button
            >
          </nav>
        {/if}
        {#if preview}
          <div class="association-preview">
            {#if preview.targetId}
              <strong>合并到「{preview.sourceName}」</strong>
              <p>
                此目录已有项目，可合并到当前项目。保留当前项目设置，以及两边的会话和草稿。
              </p>
              <div class="merge-summary">
                <span
                  >{preview.sourceName} · {preview.sourceSessions} 个会话</span
                ><span>＋</span><span
                  >{preview.targetName} · {preview.targetSessions} 个会话</span
                >
              </div>
            {:else}<strong>关联到此目录</strong>{/if}
            <code>{preview.target}</code>
            <p class="help">不会移动或删除文件。</p>
            {#if preview.targetId}
              <details>
                <summary>查看合并详情</summary>
                <dl>
                  <dt>项目名称</dt>
                  <dd>
                    {preview.alias ?? preview.target.split(/[\\/]/).at(-1)}
                  </dd>
                  <dt>默认模型</dt>
                  <dd>
                    {preview.model
                      ? `${preview.model.provider} / ${preview.model.model} · ${preview.model.thinkingLevel}`
                      : '沿用默认设置'}
                  </dd>
                  <dt>归档状态</dt>
                  <dd>{preview.archived ? '保持归档' : '在侧栏显示'}</dd>
                </dl>
                <p class="help">
                  旧项目已设置的偏好优先，独立草稿分别保留。运行中的任务结束后更新。
                </p>
              </details>
            {/if}
            <button
              class="primary"
              bind:this={action}
              disabled={busy || checking}
              onclick={() => void save()}
              >{preview.targetId ? '合并项目' : '关联目录'}</button
            >
          </div>
        {/if}
      {/if}
      <div class="operation-feedback">
        {#if busy || checking}<p role="status">
            {busy ? '正在更新项目…' : '正在检查目录…'}
          </p>
        {:else if message}<p class="location-error" role="alert">{message}</p>
          <button
            class="text-button"
            onclick={() =>
              void (waiting ? retryWaiting() : preview ? save() : check())}
            >重试</button
          >
          {#if details}<details>
              <summary>查看详情</summary>
              <pre>{details}</pre>
            </details>{/if}
        {:else if success}<p role="status">{success}</p>{/if}
      </div>
    {/if}
  </div>
</section>

<style>
  .project-location-page {
    flex: 1;
    min-height: 0;
    overflow: auto;
    padding: clamp(20px, 4vw, 48px);
  }
  .location-content {
    max-width: var(--content-width);
    margin: 0 auto;
  }
  header {
    margin-bottom: 28px;
  }
  h1 {
    font-size: 22px;
    margin: 20px 0 4px;
  }
  header p,
  .help,
  .location-current > span,
  .merge-summary,
  dt {
    color: var(--muted);
  }
  .location-current {
    margin-bottom: 28px;
  }
  code {
    display: block;
    overflow-wrap: anywhere;
    margin-top: 8px;
    font-size: 12px;
  }
  label,
  strong {
    font-weight: 600;
  }
  .help,
  p {
    margin: 8px 0 12px;
    line-height: 1.6;
  }
  .path-controls {
    display: flex;
    gap: 8px;
  }
  input {
    flex: 1;
    min-width: 0;
  }
  .directory-browser,
  .association-preview,
  .location-status {
    margin-top: 20px;
    padding-top: 20px;
    border-top: 1px solid var(--border);
  }
  .directory-heading,
  .merge-summary {
    display: flex;
    align-items: center;
    gap: 12px;
    flex-wrap: wrap;
  }
  .directory-heading code {
    flex: 1;
    min-width: 0;
  }
  ul {
    list-style: none;
    padding: 0;
    max-height: 240px;
    overflow: auto;
  }
  li button {
    display: flex;
    align-items: center;
    gap: 8px;
    width: 100%;
    text-align: left;
  }
  li span {
    overflow-wrap: anywhere;
    min-width: 0;
  }
  .empty {
    padding: 12px 0;
    color: var(--muted);
  }
  .primary {
    margin-top: 8px;
  }
  details {
    margin: 12px 0;
    color: var(--muted);
    font-size: 12px;
  }
  summary {
    cursor: pointer;
    width: fit-content;
    padding: 4px 0;
  }
  summary:focus-visible {
    outline: 2px solid var(--accent);
    outline-offset: 3px;
  }
  dl {
    display: grid;
    grid-template-columns: auto minmax(0, 1fr);
    gap: 8px 16px;
  }
  dd {
    margin: 0;
    color: var(--text);
    overflow-wrap: anywhere;
  }
  pre {
    white-space: pre-wrap;
    overflow-wrap: anywhere;
    font-size: 12px;
  }
  .operation-feedback {
    margin-top: 16px;
  }
  .location-error {
    color: var(--error);
  }
  @media (max-width: 600px) {
    .merge-summary {
      align-items: flex-start;
      gap: 6px;
    }
  }
</style>
