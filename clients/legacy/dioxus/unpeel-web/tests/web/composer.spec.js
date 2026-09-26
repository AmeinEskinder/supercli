// Playwright tests for the Composer tab of the unpeel-web component preview.
//
// What these prove: the shared `Composer` component renders in headless
// Chromium, the Send/Stop toggle flips with the simulated turn state, the
// follow-up queue accepts/edits/removes items, and drafts survive a session
// switch (all in the deterministic demo harness).
// What they do NOT prove: any real Host connection (the demo has none by
// design), native launcher behavior, or webview parity on a device.
const { test, expect } = require('@playwright/test');

test.beforeEach(async ({ page }) => {
  await page.goto('/');
  await expect(page.locator('.demo-banner')).toContainText('Web component preview', {
    timeout: 30_000,
  });
  await page.getByRole('tab', { name: 'Composer', exact: true }).click();
  await expect(page.locator('.composer-demo')).toBeVisible({ timeout: 5000 });
});

test('composer idle: send button dispatches the typed message', async ({ page }) => {
  const input = page.locator('.composer textarea');
  await expect(input).toBeVisible();
  // Idle state shows the Send control, not Stop.
  await expect(page.getByRole('button', { name: 'Send', exact: true })).toBeVisible();
  await expect(page.getByRole('button', { name: 'Stop', exact: true })).toHaveCount(0);

  await input.fill('hello agent');
  await page.getByRole('button', { name: 'Send', exact: true }).click();
  await expect(page.getByTestId('sent-log')).toContainText('sent: 1');
  await expect(page.locator('.sent-msg', { hasText: 'hello agent' })).toBeVisible();
  // The input clears after sending.
  await expect(input).toHaveValue('');
});

test('composer running: stop button interrupts and queue accepts follow-ups', async ({ page }) => {
  // Simulate a running turn; the control must flip to Stop.
  await page.getByTestId('turn-toggle').click();
  await expect(page.getByRole('button', { name: 'Stop', exact: true })).toBeVisible();
  await expect(page.getByRole('button', { name: 'Send', exact: true })).toHaveCount(0);

  // While running, the composer input queues follow-ups instead of sending.
  const input = page.locator('.composer textarea');
  await input.fill('follow-up one');
  await page.getByRole('button', { name: 'Queue', exact: true }).click();
  await expect(page.locator('.followup-queue')).toContainText('follow-up one');
  await expect(input).toHaveValue('');

  // Queued items are editable: open edit, change text, save.
  await page.getByRole('button', { name: 'Edit', exact: true }).click();
  const editInput = page.locator('.followup-queue input[type="text"], .followup-queue textarea').first();
  await editInput.fill('follow-up one (edited)');
  await page.getByRole('button', { name: 'Save', exact: true }).click();
  await expect(page.locator('.followup-queue')).toContainText('follow-up one (edited)');

  // And removable.
  await page.getByRole('button', { name: 'Remove', exact: true }).click();
  await expect(page.locator('.followup-queue')).toHaveCount(0);

  // Stop interrupts the turn: the control flips back to Send.
  await page.getByRole('button', { name: 'Stop', exact: true }).click();
  await expect(page.getByTestId('sent-log')).toContainText('stops: 1');
  await expect(page.getByRole('button', { name: 'Send', exact: true })).toBeVisible();
});

test('drafts survive a session switch', async ({ page }) => {
  const input = page.locator('.composer textarea');
  await input.fill('draft for session one');
  // Switch sessions: the draft for session one is saved, the new session
  // starts with an empty composer.
  await page.getByTestId('session-toggle').click();
  await expect(input).toHaveValue('');
  await input.fill('draft for session two');
  // Switch back: session one's draft is restored.
  await page.getByTestId('session-toggle').click();
  await expect(input).toHaveValue('draft for session one');
  // And forward again restores session two's draft.
  await page.getByTestId('session-toggle').click();
  await expect(input).toHaveValue('draft for session two');
});
