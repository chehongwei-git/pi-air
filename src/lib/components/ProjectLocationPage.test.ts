/** 目录关联默认只读预览，合并必须由明确操作提交；诊断不挤占正常提示。 */
import { render, fireEvent, waitFor, cleanup } from '@testing-library/svelte';
import { afterEach, expect, it, vi } from 'vitest';
import ProjectLocationPage from './ProjectLocationPage.svelte';
import type { Workspace } from '../state/workspace.svelte';
import type { ProjectMergePreview } from '../types';
afterEach(() => {
  cleanup();
  vi.useRealTimers();
});

function fixture(merge = false) {
  const mergeProjects = vi.fn(async (_preview: ProjectMergePreview) => {});
  const previewProjectMerge = vi.fn(
    async (_id: string, target: string): Promise<ProjectMergePreview> => ({
      id: 'p1',
      target,
      targetId: merge ? 'p2' : null,
      directoryIdentity: 'native:1',
      revision: 3,
      sourceName: '项目别名',
      targetName: merge ? '新项目' : null,
      sourceSessions: 5,
      targetSessions: merge ? 2 : 0,
      automatic: false,
      alias: '项目别名',
      model: null,
      archived: false,
    }),
  );
  const workspace = {
    active: {
      settings: {
        projectRevision: 3,
        projects: [{ id: 'p1', currentPath: '/work/old' }],
      },
      projectName: () => '项目别名',
    },
    previewProjectMerge,
    mergeProjects,
    browseDirectories: vi.fn(async (path: string) => ({
      path,
      parent: '/work',
      directories: ['/work/new'],
    })),
    cancelProjectMerge: vi.fn(async () => {}),
  } as unknown as Workspace;
  const view = render(ProjectLocationPage, {
    workspace,
    projectId: 'p1',
    onclose: vi.fn(),
  });
  return { view, mergeProjects, previewProjectMerge };
}
it('automatically previews the path but submits only through the single inline action', async () => {
  const { view, mergeProjects, previewProjectMerge } = fixture();
  await fireEvent.input(view.getByLabelText('重新关联目录'), {
    target: { value: '/work/new' },
  });
  const button = await view.findByRole('button', {
    name: '关联目录',
  });
  expect(previewProjectMerge).toHaveBeenCalledWith('p1', '/work/new');
  expect(mergeProjects).not.toHaveBeenCalled();
  expect(view.queryByRole('dialog')).toBeNull();
  expect(view.queryByRole('button', { name: '检查关联' })).toBeNull();
  await fireEvent.click(button);
  await waitFor(() =>
    expect(mergeProjects).toHaveBeenCalledWith(
      expect.objectContaining({ id: 'p1', target: '/work/new', revision: 3 }),
    ),
  );
  expect(view.getByText('目录已更新。')).toBeTruthy();
});
it('shows a neutral merge preview and keeps diagnostics folded after a failed save', async () => {
  const { view, mergeProjects } = fixture(true);
  mergeProjects.mockRejectedValueOnce({
    code: 'IO',
    message: '磁盘无法写入',
    diagnosticId: '20628-15',
  });
  const input = view.getByLabelText('重新关联目录') as HTMLInputElement;
  await fireEvent.input(input, { target: { value: '/work/new' } });
  const button = await view.findByRole('button', {
    name: '合并项目',
  });
  expect(view.queryByRole('alert')).toBeNull();
  expect(view.getByText('新项目 · 2 个会话')).toBeTruthy();
  await fireEvent.click(button);
  await waitFor(() =>
    expect(view.getByRole('alert').textContent).toBe(
      '没有保存成功，输入已保留',
    ),
  );
  expect(view.getByRole('alert').textContent).not.toContain('20628-15');
  const details = view.getByText('查看详情').closest('details')!;
  expect(details.open).toBe(false);
  expect(details.textContent).toContain('20628-15');
  expect(input.value).toBe('/work/new');
  expect(document.activeElement).toBe(button);
  await fireEvent.click(view.getByRole('button', { name: '重试' }));
  await waitFor(() => expect(view.getByText('项目已合并。')).toBeTruthy());
});
it('ignores an older path check that completes after a newer result', async () => {
  const { view, previewProjectMerge } = fixture();
  const original = previewProjectMerge.getMockImplementation()!;
  let release!: (value: ProjectMergePreview) => void;
  previewProjectMerge.mockImplementationOnce(
    () =>
      new Promise((resolve) => {
        release = resolve;
      }),
  );
  const input = view.getByLabelText('重新关联目录');
  await fireEvent.input(input, { target: { value: '/work/first' } });
  await waitFor(() => expect(previewProjectMerge).toHaveBeenCalledTimes(1));
  await fireEvent.input(input, { target: { value: '/work/second' } });
  await view.findByRole('button', { name: '关联目录' });
  release(await original('p1', '/work/first'));
  await waitFor(() => expect(view.getByText('/work/second')).toBeTruthy());
  expect(view.queryByText('/work/first')).toBeNull();
});
it('defers metadata checks until IME composition ends', async () => {
  vi.useFakeTimers();
  const { view, previewProjectMerge } = fixture();
  const input = view.getByLabelText('重新关联目录');
  await fireEvent.compositionStart(input);
  await fireEvent.input(input, { target: { value: '/work/中文' } });
  await vi.advanceTimersByTimeAsync(500);
  expect(previewProjectMerge).not.toHaveBeenCalled();
  await fireEvent.compositionEnd(input);
  await vi.advanceTimersByTimeAsync(350);
  expect(previewProjectMerge).toHaveBeenCalledWith('p1', '/work/中文');
});
