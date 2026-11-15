import { test, expect, type Page } from '@playwright/test';

async function setup(page: Page, query = '') {
  await page.route(/\/bookmarks-fixture(?:\?.*)?$/, (route) =>
    route.fulfill({
      contentType: 'text/html',
      body: '<!doctype html><html><head><meta charset="utf-8"></head><body><main style="height:100%;display:flex;flex-direction:column"></main><script type="module" src="/tests/browser/bookmarks-fixture.ts"></script></body></html>',
    }),
  );
  await page.goto('/bookmarks-fixture' + query);
  await expect(
    page.getByRole('button', { name: '浏览提问目录' }),
  ).toBeVisible();
}

for (const narrow of [false, true]) {
  test(`hover directory preserves layout, scrolls independently and locates history (${narrow ? 'narrow dark' : 'wide light'})`, async ({
    page,
  }) => {
    if (narrow) await page.setViewportSize({ width: 480, height: 680 });
    await setup(page);
    if (narrow)
      await page.evaluate(() => {
        document.documentElement.dataset.theme = 'dark';
        document.documentElement.style.zoom = '1.25';
      });
    await page.getByRole('textbox', { name: '草稿' }).fill('不要丢失的草稿');
    const transcript = page.locator('.transcript');
    await expect
      .poll(() =>
        transcript.evaluate(
          (node) => node.scrollHeight - node.scrollTop - node.clientHeight,
        ),
      )
      .toBeLessThan(3);
    const before = await transcript.boundingBox();
    const draft = await page.getByRole('textbox').boundingBox();
    const top = await transcript.evaluate((node) => node.scrollTop);
    const trigger = page.getByRole('button', { name: '浏览提问目录' });
    await trigger.hover();
    await expect(page.locator('.bookmark-surface')).toBeVisible();
    await expect(page.locator('.bookmark-item')).toHaveCount(40);
    await expect(
      page.getByRole('button', { name: '加载更早提问', exact: true }),
    ).toHaveCount(0);
    expect(await transcript.boundingBox()).toEqual(before);
    expect(await page.getByRole('textbox').boundingBox()).toEqual(draft);
    expect(await transcript.evaluate((node) => node.scrollTop)).toBe(top);
    await page.locator('.bookmark-list').hover();
    await page.mouse.wheel(0, -800);
    await expect(page.locator('.bookmark-item')).toHaveCount(40);
    expect(await transcript.evaluate((node) => node.scrollTop)).toBe(top);
    expect(await page.locator('[data-message-id]').count()).toBe(40);
    await page.locator('.bookmark-list').evaluate((node) => {
      node.scrollTop = 0;
    });
    await expect(page.locator('.bookmark-item')).toHaveCount(40);
    const target = page.locator('[data-bookmark-id="bookmark-0"]');
    await target.click();
    await expect(page.locator('.bookmark-surface')).toBeHidden();
    await expect(page.locator('[data-message-id]')).toHaveCount(80);
    await expect
      .poll(async () => {
        const node = await page
          .locator('[data-message-id="bookmark-0"]')
          .boundingBox();
        const area = await transcript.boundingBox();
        return Math.abs(node!.y - area!.y - (narrow ? 30 : 24));
      })
      .toBeLessThan(3);
    const anchored = await transcript.evaluate((node) => node.scrollTop);
    await page.evaluate(() => (window as any).append());
    await expect(page.locator('[data-message-id]')).toHaveCount(81);
    expect(await transcript.evaluate((node) => node.scrollTop)).toBe(anchored);
    await expect(page.getByRole('textbox')).toHaveValue('不要丢失的草稿');
    await page.screenshot({ path: test.info().outputPath('located.png') });
    await page.mouse.move(10, 10);
    await trigger.hover();
    await expect(page.locator('.bookmark-surface')).toBeVisible();
    // 截图记录稳定终态，不把淡入中间帧误作最终配色或文字重影。
    await expect(page.locator('.bookmark-surface')).toHaveCSS('opacity', '1');
    await page.screenshot({ path: test.info().outputPath('directory.png') });
    const box = (await page.locator('.bookmark-surface').boundingBox())!;
    expect(box.x).toBeGreaterThanOrEqual(0);
    expect(box.y).toBeGreaterThanOrEqual(0);
    expect(box.x + box.width).toBeLessThanOrEqual(narrow ? 480 : 1200);
    await page.mouse.move(10, 10);
    await expect(page.locator('.bookmark-surface')).toBeHidden();
    await trigger.focus();
    await trigger.press('Enter');
    await page.keyboard.press('Escape');
    await expect(page.locator('.bookmark-surface')).toBeHidden();
  });
}

test('complete directory loads without hover, caches requests and virtualizes ten thousand prompts', async ({
  page,
}) => {
  await setup(page, '?large');
  await expect
    .poll(() => page.evaluate(() => (window as any).historyCalls))
    .toBe(40);
  await page.waitForTimeout(150);
  expect(await page.evaluate(() => (window as any).historyCalls)).toBe(40);
  expect(await page.locator('[data-message-id]').count()).toBe(40);
  const trigger = page.getByRole('button', { name: '浏览提问目录' });
  await trigger.focus();
  await trigger.press('Enter');
  await page.keyboard.press('Home');
  await expect(page.locator('[data-bookmark-id="large-0"]')).toBeFocused();
  expect(await page.locator('.bookmark-item').count()).toBeLessThanOrEqual(24);
  await page.keyboard.press('End');
  await expect(page.locator('[data-bookmark-id="bookmark-78"]')).toBeFocused();
  expect(await page.locator('.bookmark-item').count()).toBeLessThanOrEqual(24);
  await page.keyboard.press('Escape');
  await trigger.press('Enter');
  expect(await page.evaluate(() => (window as any).historyCalls)).toBe(40);
  await page.evaluate(() => (window as any).append());
  expect(await page.evaluate(() => (window as any).historyCalls)).toBe(40);
  await expect(page.locator('.bookmark-surface')).toHaveCSS('opacity', '1');
  await page.screenshot({
    path: test.info().outputPath('large-directory.png'),
  });
});

test('hover timing and node churn diagnostic for a complete large directory', async ({
  page,
}) => {
  await setup(page, '?large');
  await expect
    .poll(() => page.evaluate(() => (window as any).historyCalls))
    .toBe(40);
  const samples = await page.evaluate(async () => {
    const nav = document.querySelector<HTMLElement>('.bookmarks')!;
    const samples: { latency: number; added: number; removed: number }[] = [];
    for (let index = 0; index < 4; index++) {
      let added = 0;
      let removed = 0;
      const observer = new MutationObserver((records) => {
        for (const record of records) {
          added += record.addedNodes.length;
          removed += record.removedNodes.length;
        }
      });
      observer.observe(nav, { childList: true, subtree: true });
      const start = performance.now();
      nav.dispatchEvent(new PointerEvent('pointerenter'));
      await new Promise<void>((resolve) => {
        const sample = () => {
          const surface = nav.querySelector<HTMLElement>('.bookmark-surface');
          if (
            nav.classList.contains('expanded') &&
            surface &&
            Number(getComputedStyle(surface).opacity) > 0
          )
            resolve();
          else requestAnimationFrame(sample);
        };
        requestAnimationFrame(sample);
      });
      const latency = performance.now() - start;
      nav.dispatchEvent(new PointerEvent('pointerleave'));
      await new Promise((resolve) => setTimeout(resolve, 400));
      observer.disconnect();
      samples.push({ latency, added, removed });
    }
    return samples;
  });
  await test.info().attach('hover-samples', {
    body: JSON.stringify(samples),
    contentType: 'application/json',
  });
  console.log('bookmark hover samples', JSON.stringify(samples));
  expect(samples).toHaveLength(4);
  // 首次切换到当前虚拟窗口允许变更；之后重复开关不得重建缓存节点。
  for (const sample of samples.slice(1)) {
    expect(sample.added).toBe(0);
    expect(sample.removed).toBe(0);
  }
  expect(await page.evaluate(() => (window as any).historyCalls)).toBe(40);
});

test('cached surface is inert when closed and rapid reentry keeps one prepared view', async ({
  page,
}) => {
  await setup(page);
  const trigger = page.getByRole('button', { name: '浏览提问目录' });
  const surface = page.locator('.bookmark-surface');
  await trigger.hover();
  await expect(trigger).toHaveAttribute('aria-expanded', 'true');
  await expect(surface).toHaveAttribute('aria-hidden', 'false');
  await page.evaluate(() => {
    (window as any).cachedSurface = document.querySelector('.bookmark-surface');
  });
  const area = (await surface.boundingBox())!;
  await page.mouse.move(10, 10);
  // 使用原始鼠标坐标立即返回，不让自动化的稳定性等待耗尽关闭宽限。
  await page.mouse.move(area.x + area.width / 2, area.y + area.height / 2);
  await expect(trigger).toHaveAttribute('aria-expanded', 'true');
  expect(
    await page.evaluate(
      () =>
        (window as any).cachedSurface ===
        document.querySelector('.bookmark-surface'),
    ),
  ).toBe(true);
  await page.mouse.move(10, 10);
  await expect(surface).toBeHidden();
  await expect(surface).toHaveAttribute('inert', '');
  await expect(surface).toHaveAttribute('aria-hidden', 'true');
  await page.evaluate(() => {
    document.querySelector<HTMLButtonElement>('.bookmark-item')!.focus();
  });
  expect(
    await page
      .locator('.bookmark-item')
      .first()
      .evaluate((node) => node === document.activeElement),
  ).toBe(false);
  await page.emulateMedia({ reducedMotion: 'reduce' });
  await trigger.focus();
  await trigger.press('Enter');
  await expect(surface).toBeVisible();
  expect(
    await surface.evaluate((node) => getComputedStyle(node).transitionDuration),
  ).toBe('0s');
  expect(
    await page.evaluate(
      () =>
        (window as any).cachedSurface ===
        document.querySelector('.bookmark-surface'),
    ),
  ).toBe(true);
  await page.keyboard.press('Escape');
  await expect(trigger).toBeFocused();
});

test('keyboard, retry, stale session response and cancellation', async ({
  page,
}) => {
  await setup(page, '?fail');
  const trigger = page.getByRole('button', { name: '浏览提问目录' });
  await trigger.focus();
  await trigger.press('Enter');
  await expect(page.getByRole('alert')).toContainText('模拟目录读取失败');
  await page.evaluate(() => {
    (window as any).failHistory = false;
  });
  await page.getByRole('button', { name: '重试', exact: true }).click();
  await expect(page.getByRole('alert')).toHaveCount(0);
  await page.locator('[data-bookmark-id]').first().focus();
  await page.keyboard.press('End');
  await expect(page.locator('[data-bookmark-id]').last()).toBeFocused();
  await page.keyboard.press('Escape');
  await expect(trigger).toBeFocused();
  await expect(page.locator('.bookmark-surface')).toBeHidden();
  await page.evaluate(() => {
    (window as any).delay = true;
  });
  await page.evaluate(() => {
    (window as any).switchSession();
  });
  await expect
    .poll(() => page.evaluate(() => typeof (window as any).release))
    .toBe('function');
  await page.evaluate(() => {
    (window as any).setActive(false);
    (window as any).release();
  });
  await page.evaluate(() => {
    (window as any).delay = false;
    (window as any).setActive(true);
  });
  await expect(page.locator('.bookmark-surface')).toBeHidden();
  await trigger.press('Enter');
  await expect(page.locator('.bookmark-item')).toHaveCount(1);
  await expect(page.locator('.bookmark-item')).toContainText('另一个会话');
  await page.keyboard.press('Escape');
});
