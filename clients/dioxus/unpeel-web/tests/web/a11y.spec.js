// Accessibility tests for the unpeel-web component preview.
//
// What these prove:
//  1. axe-core finds 0 serious/critical violations on every demo tab
//     (WCAG 2.0/2.1 A+AA rules).
//  2. The approval flow is fully keyboard-operable: Tab reaches every
//     control, Enter approves/denies a review, Enter cancels a running turn.
// What they do NOT prove: screen-reader UX (no AT in headless Chromium),
// native launcher behavior, or a real Host connection (demo by design).
const { test, expect } = require('@playwright/test');
const { AxeBuilder } = require('@axe-core/playwright');

test.beforeEach(async ({ page }) => {
  await page.goto('/');
  await expect(page.locator('.demo-banner')).toContainText('Web component preview', {
    timeout: 30_000,
  });
});

test('axe: 0 serious/critical violations on every demo tab', async ({ page }) => {
  const tabs = [
    'Pairing',
    'Sessions',
    'Terminal',
    'Composer',
    'Approvals',
    'Gallery',
    'Dictation',
    'Toasts & Find',
  ];
  for (const tab of tabs) {
    await page.getByRole('tab', { name: tab, exact: true }).click();
    // Let the freshly mounted tab settle (autofocus, effects).
    await page.waitForTimeout(400);
    const results = await new AxeBuilder({ page }).analyze();
    const bad = results.violations.filter(
      (v) => v.impact === 'serious' || v.impact === 'critical',
    );
    expect(
      bad,
      `tab "${tab}" violations: ${JSON.stringify(bad.map((v) => v.id))}`,
    ).toEqual([]);
  }
});

// Press Tab until the focused element's accessible name contains `name`.
// Keyboard-only by construction: no clicks, no programmatic focus().
async function tabTo(page, name, max = 80) {
  for (let i = 0; i < max; i++) {
    const focused = await page.evaluate(() => {
      const el = document.activeElement;
      if (!el || el === document.body) return '';
      return (el.getAttribute('aria-label') || el.textContent || '').trim();
    });
    if (focused.includes(name)) return;
    await page.keyboard.press('Tab');
  }
  throw new Error(`keyboard-only navigation never reached "${name}"`);
}

async function focusedName(page) {
  return page.evaluate(() => {
    const el = document.activeElement;
    return el ? (el.getAttribute('aria-label') || el.textContent || '').trim() : '';
  });
}

test('keyboard-only: approve a review', async ({ page }) => {
  await page.getByRole('tab', { name: 'Approvals', exact: true }).click();

  await tabTo(page, 'Simulate approval request');
  await page.keyboard.press('Enter');

  // The alertdialog mounts and moves focus to Approve.
  await expect(page.getByRole('alertdialog')).toBeVisible();
  expect(await focusedName(page)).toContain('Approve');

  await page.keyboard.press('Enter');
  await expect(page.getByTestId('approval-status')).toContainText('Approved:');
});

test('keyboard-only: deny a review and cancel a running turn', async ({ page }) => {
  await page.getByRole('tab', { name: 'Approvals', exact: true }).click();

  await tabTo(page, 'Simulate approval request');
  await page.keyboard.press('Enter');
  await expect(page.getByRole('alertdialog')).toBeVisible();

  // Tab from Approve to Deny, then activate with the keyboard.
  await page.keyboard.press('Tab');
  expect(await focusedName(page)).toContain('Deny');
  await page.keyboard.press('Enter');
  await expect(page.getByTestId('approval-status')).toContainText('Denied:');

  // Cancel a running turn, keyboard only.
  await tabTo(page, 'Start simulated turn');
  await page.keyboard.press('Enter');
  await expect(page.getByTestId('cancel-turn')).toBeVisible();

  await tabTo(page, 'Cancel turn');
  await page.keyboard.press('Enter');
  await expect(page.getByTestId('approval-status')).toContainText('Turn cancelled.');
});

test('keyboard-only: composer sends with Enter', async ({ page }) => {
  await page.getByRole('tab', { name: 'Composer', exact: true }).click();

  await tabTo(page, 'Message the agent');
  await page.keyboard.type('keyboard-only message');
  await page.keyboard.press('Enter');
  await expect(page.getByTestId('sent-log')).toContainText('sent: 1');
});
