/** 内嵌合并在明暗主题、长路径和窄窗口中保持精简，并保留失败输入与会话草稿。 */
import { test, expect } from '@playwright/test';

for (const theme of ['light', 'dark']) {
  test(`merges an occupied directory inline in ${theme} theme`, async ({
    page,
  }) => {
    const target = `/work/已改名的项目-${'很长的目录名称'.repeat(10)}`;
    await page.addInitScript(
      ({ theme, target }) => {
        const settings = {
          schemaVersion: 1,
          piPath: '',
          runtimePath: '',
          extraPath: [],
          sessionDir: '',
          recentProjects: ['/work/old', target],
          projectOrder: ['/work/old', target],
          pinnedSessions: [],
          sidebarItems: [],
          theme,
          zoom: 100,
          projectRevision: 1,
          projects: [
            {
              id: 'p1',
              currentPath: '/work/old',
              historicalPaths: [] as string[],
              directoryIdentity: 'test:1',
              sessions: [],
              mergedIds: [] as string[],
            },
            {
              id: 'p2',
              currentPath: target,
              historicalPaths: [] as string[],
              directoryIdentity: 'test:2',
              sessions: [],
              mergedIds: [] as string[],
            },
          ],
        };
        let fail = true;
        (window as any).__TAURI_INTERNALS__ = {
          transformCallback: () => 1,
          unregisterCallback: () => {},
          async invoke(command: string, args: any) {
            if (command === 'get_settings') return structuredClone(settings);
            if (command === 'preview_project_merge')
              return {
                id: 'p1',
                target: args.target,
                targetId: 'p2',
                directoryIdentity: 'test:2',
                revision: settings.projectRevision,
                sourceName: '原项目',
                targetName: '手动新建的项目',
                sourceSessions: 12,
                targetSessions: 2,
                automatic: false,
                alias: '原项目',
                model: {
                  project: '/work/old',
                  provider: 'test',
                  model: '旧项目模型',
                  thinkingLevel: 'high',
                },
                archived: false,
              };
            if (command === 'merge_projects') {
              if (fail) {
                fail = false;
                throw {
                  code: 'IO',
                  message: '测试：目录暂时不可写',
                  diagnosticId: '20628-15',
                };
              }
              settings.projects[0].currentPath = args.request.target;
              settings.projects[0].historicalPaths = ['/work/old'];
              settings.projects[0].mergedIds = ['p2'];
              settings.projects = settings.projects.filter(
                (p) => p.id === 'p1',
              );
              settings.recentProjects = [args.request.target];
              settings.projectOrder = [args.request.target];
              settings.projectRevision++;
            }
            if (
              command === 'refresh_project_locations' ||
              command === 'merge_projects'
            )
              return {
                settings: structuredClone(settings),
                locations: [
                  {
                    id: 'p1',
                    currentPath: settings.projects[0].currentPath,
                    state:
                      settings.projectRevision === 1
                        ? 'unavailable'
                        : 'available',
                    candidate: null,
                    message:
                      settings.projectRevision === 1 ? '需要关联目录' : '',
                  },
                ],
              };
            if (command === 'browse_project_directories')
              return {
                path: args.path,
                parent: '/work',
                directories: [`${args.path}/子目录`],
              };
            if (command === 'list_sessions' || command === 'list_activity')
              return [];
            if (command === 'connect_pi') throw new Error('测试不启动 Pi');
            return null;
          },
        };
      },
      { theme, target },
    );
    await page.goto('/');
    await page
      .getByRole('button', { name: '在 /work/old 中新建会话', exact: true })
      .click();
    const draft = page.getByRole('textbox', { name: '消息输入', exact: true });
    await draft.fill('合并后继续编辑');
    await page
      .getByRole('button', { name: '需要关联目录', exact: true })
      .click();
    const panel = page.getByRole('region', { name: '项目目录关联' });
    await expect(panel).toBeVisible();
    await expect(page.getByRole('dialog')).toHaveCount(0);
    const path = page.getByLabel('重新关联目录', { exact: true });
    await expect(path).toBeFocused();
    await path.fill(target);
    const merge = page.getByRole('button', { name: '合并项目', exact: true });
    await expect(merge).toBeVisible();
    await expect(page.getByRole('button', { name: '检查关联' })).toHaveCount(0);
    await expect(page.getByRole('alert')).toHaveCount(0);
    await page.setViewportSize({ width: 760, height: 700 });
    await page
      .getByRole('complementary', { name: '项目与历史会话' })
      .getByRole('button', { name: '收起侧栏', exact: true })
      .click();
    await page.screenshot({
      path: `test-results/project-location-${theme}.png`,
      fullPage: true,
    });
    expect(
      await panel.evaluate((node) => node.scrollWidth <= node.clientWidth),
    ).toBe(true);
    await merge.click();
    await expect(page.getByRole('alert')).toHaveText(
      '没有保存成功，输入已保留',
    );
    await expect(merge).toBeFocused();
    await expect(page.getByText(/诊断 20628-15/)).not.toBeVisible();
    await page.getByText('查看详情', { exact: true }).click();
    await expect(page.getByText(/诊断 20628-15/)).toBeVisible();
    await expect(path).toHaveValue(target);
    await page.getByRole('button', { name: '重试', exact: true }).click();
    await expect(page.getByText('项目已合并。')).toBeVisible();
    await page.getByRole('button', { name: '返回会话' }).click();
    await expect(draft).toHaveValue('合并后继续编辑');
    await expect(draft).toBeFocused();
  });
}
