// Playwright smoke tests for the unpeel-web Dioxus component preview.
//
// What these prove: the wasm bundle loads in headless Chromium and the
// shared unpeel-ui components render against the scripted demo state.
// What they do NOT prove: any real Host connection (the demo has none by
// design), native launcher behavior, or webview parity on a device.
const { test, expect } = require('@playwright/test');

test.beforeEach(async ({ page }) => {
  await page.goto('/');
  // The Dioxus app hydrates async; the banner is rendered by the app shell.
  await expect(page.locator('.demo-banner')).toContainText('Web component preview', {
    timeout: 30_000,
  });
});

test('pairing tab renders the demo hosts', async ({ page }) => {
  await expect(page.getByRole('heading', { name: 'Pair with an Unpeel Host' })).toBeVisible();
  await expect(page.locator('.host-name', { hasText: 'Demo Mac' })).toBeVisible();
  // Demo pairing is honest about being a demo: the demo code is accepted
  // and transitions to the Sessions tab, like the real launcher does after
  // pairing; anything else fails with an error.
  await page.locator('.pairing-code').fill('UNPEEL:1:demo');
  await page.getByRole('button', { name: 'Pair', exact: true }).click();
  await expect(page.getByTestId('session-list')).toBeVisible({ timeout: 5000 });
  // A bogus code fails honestly (reload so the demo code's paired state
  // doesn't leave the pairing view behind).
  await page.reload();
  await expect(page.locator('.demo-banner')).toContainText('Web component preview', {
    timeout: 30_000,
  });
  await page.locator('.pairing-code').fill('bogus-code');
  await page.getByRole('button', { name: 'Pair', exact: true }).click();
  await expect(page.locator('.error')).toContainText('Demo build: pairing needs a real Host.', { timeout: 5000 });
});

test('sessions tab renders the demo session list', async ({ page }) => {
  await page.getByRole('tab', { name: 'Sessions' }).click();
  const list = page.locator('.session-list');
  await expect(list).toContainText('harness build');
  await expect(list).toContainText('docs pass');
});

test('terminal tab renders scripted output and echoes keys', async ({ page }) => {
  await page.getByRole('tab', { name: 'Terminal' }).click();
  const term = page.locator('.terminal-view');
  await expect(term).toContainText('unpeel status');
  await expect(term).toContainText('demo-mac-1');
  // The demo feeds typed keys back through the real VT parser (echo).
  await term.click();
  await page.keyboard.type('echo hi');
  await expect(term).toContainText('echo hi');
});

test('extras tab: toast shows and dismisses on tap', async ({ page }) => {
  await page.getByRole('tab', { name: 'Toasts & Find' }).click();
  await page.getByRole('button', { name: 'Show toast' }).click();
  const toast = page.locator('.toast-overlay');
  const capsule = page.locator('.toast-capsule');
  await expect(toast).toContainText('Demo toast from the shared ToastCenter.');
  // Click the capsule itself: the overlay is pointer-events:none by design,
  // only the capsule receives pointer events.
  await capsule.click();
  await expect(toast).toHaveCount(0);
});

test('extras tab: find bar searches the terminal snapshot', async ({ page }) => {
  await page.getByRole('tab', { name: 'Toasts & Find' }).click();
  await page.locator('.find-field').fill('unpeel');
  // The counter reflects the shared FindState over the demo snapshot.
  await expect(page.locator('.find-count')).not.toHaveText('');
});
