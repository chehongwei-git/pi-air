/** 已有目录身份的自动合并不打开管理页、不改变输入焦点或当前草稿。 */
import { test, expect } from '@playwright/test';

test('absorbs an identified duplicate in the background without interrupting the conversation', async ({
  page,
}) => {
  await page.addInitScript(() => {
    const record = (id: string, path: string) => ({
      id,
      currentPath: path,
      historicalPaths: [] as string[],
      directoryIdentity: 'native:same',
      sessions: [] as string[],
      mergedIds: [] as string[],
    });
    const settings = {
      schemaVersion: 1,
      piPath: '',
      runtimePath: '',
      extraPath: [],
      sessionDir: '',
      recentProjects: ['/work/old', '/work/new'],
      projectOrder: ['/work/old', '/work/new'],
      pinnedSessions: [],
      sidebarItems: [],
      theme: 'light',
      zoom: 100,
      projectRevision: 1,
      projects: [record('old', '/work/old'), record('new', '/work/new')],
    };
    (window as any).__renameReady = false;
    (window as any).__manualMergeCalls = 0;
    (window as any).__TAURI_INTERNALS__ = {
      transformCallback: () => 1,
      unregisterCallback: () => {},
      async invoke(command: string) {
        if (command === 'get_settings') return structuredClone(settings);
        if (command === 'merge_projects') {
          (window as any).__manualMergeCalls++;
          return null;
        }
        if (command === 'refresh_project_locations') {
          if ((window as any).__renameReady && settings.projects.length === 2) {
            settings.projects[0] = {
              ...record('old', '/work/new'),
              historicalPaths: ['/work/old'],
              mergedIds: ['new'],
            };
            settings.projects.pop();
            settings.projectOrder = ['/work/new'];
            settings.recentProjects = ['/work/new'];
            settings.projectRevision++;
          }
          return {
            settings: structuredClone(settings),
            locations: settings.projects.map((p) => ({
              id: p.id,
              currentPath: p.currentPath,
              state: 'available',
              candidate: null,
              message: '',
            })),
          };
        }
        if (command === 'list_sessions' || command === 'list_activity')
          return [];
        if (command === 'connect_pi') throw new Error('测试不启动 Pi');
        return null;
      },
    };
  });
  await page.goto('/');
  // 禁用真实 Pi 后初始连接失败；显式准备原项目本地草稿，建立要保留的目录上下文。
  await page
    .getByRole('button', { name: '在 /work/old 中新建会话', exact: true })
    .click();
  const draft = page.getByRole('textbox', { name: '消息输入', exact: true });
  await draft.fill('改名后保持当前输入');
  await page.evaluate(() => {
    (window as any).__renameReady = true;
  });
  await expect
    .poll(() => page.evaluate(() => (window as any).__manualMergeCalls))
    .toBe(0);
  // 以侧栏项目入口数量核验轮询已生效，再断言无额外管理界面和焦点抢占。
  await expect.poll(() => page.locator('.project-group').count()).toBe(1);
  await expect(draft).toHaveValue('改名后保持当前输入');
  await expect(draft).toBeFocused();
  await expect(page.getByRole('region', { name: '项目目录关联' })).toHaveCount(
    0,
  );
  await expect(page.getByRole('dialog')).toHaveCount(0);
  expect(await page.evaluate(() => (window as any).__manualMergeCalls)).toBe(0);
});
