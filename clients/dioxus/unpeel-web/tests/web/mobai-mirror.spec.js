// Playwright mirrors of the five MobAI device flows (tests/device/*.mob),
// rewritten around stable data-testid selectors instead of text labels.
//
// All five flows are executable against the web demo build. The demo uses
// scripted state (no Host) but exercises the real shared unpeel-ui components
// with the same test IDs the device flows use. The flows are faithful:
// pairing transitions to the Sessions tab, opening a session navigates to
// its Terminal, and the gallery flow goes detail → Draw editor →
// annotation-done.

const { test, expect } = require('@playwright/test');

test.beforeEach(async ({ page }) => {
  await page.goto('/');
  await expect(page.locator('.demo-banner')).toContainText('Web component preview', {
    timeout: 30_000,
  });
});

// Mirror of pair.mob: pairing screen renders with test IDs. The demo accepts
// the demo code (UNPEEL:1:demo) and transitions to the Sessions tab, like the
// real mobile launcher does after pairing; other codes fail honestly.
test('mobai mirror: pair flow', async ({ page }) => {
  const view = page.getByTestId('pairing-view');
  await expect(view).toBeVisible();
  await expect(page.getByTestId('pairing-title')).toBeVisible();

  const input = page.getByTestId('pairing-code-input');
  await expect(input).toBeVisible();
  await input.fill('UNPEEL:1:demo');

  const submit = page.getByTestId('pairing-submit');
  await expect(submit).toBeEnabled();
  await submit.click();
  // Faithful: successful pairing transitions to the Sessions tab.
  const list = page.getByTestId('session-list');
  await expect(list).toBeVisible({ timeout: 5000 });
});

// Mirror of open-session.mob: session list renders with test IDs; rows are
// addressable by stable id. Tapping a row selects it AND navigates to its
// Terminal, like the real launcher.
test('mobai mirror: open-session flow', async ({ page }) => {
  // Pair first to reach the Sessions tab (faithful entry point).
  await page.getByTestId('pairing-code-input').fill('UNPEEL:1:demo');
  await page.getByTestId('pairing-submit').click();
  const list = page.getByTestId('session-list');
  await expect(list).toBeVisible({ timeout: 5000 });

  const row = page.getByTestId('session-row-sess-demo-2');
  await expect(row).toBeVisible();
  await expect(row.getByTestId('session-title')).toContainText('docs pass');
  await row.click();
  // Faithful: opening a session navigates to its Terminal.
  const term = page.getByTestId('terminal-view');
  await expect(term).toBeVisible({ timeout: 5000 });
  // The terminal is showing the selected session.
  await expect(page.getByTestId('terminal-session-id')).toContainText('sess-demo-2');
});

// Mirror of terminal-type.mob: terminal view renders with test ID and echoes
// typed keys through the real VT parser. Reached via the faithful
// pair → sessions → open-session path.
test('mobai mirror: terminal-type flow', async ({ page }) => {
  // Pair, then open sess-demo-1's terminal via the session list.
  await page.getByTestId('pairing-code-input').fill('UNPEEL:1:demo');
  await page.getByTestId('pairing-submit').click();
  await expect(page.getByTestId('session-list')).toBeVisible({ timeout: 5000 });
  await page.getByTestId('session-row-sess-demo-1').click();

  const term = page.getByTestId('terminal-view');
  await expect(term).toBeVisible({ timeout: 5000 });
  await expect(term).toContainText('unpeel status');
  await term.click();
  await page.keyboard.type('echo unpeel-smoke-test');
  await expect(term).toContainText('echo unpeel-smoke-test');
});

// Mirror of gallery-annotate.mob: gallery panel renders with test IDs.
// Screenshot adds a demo entry; opening it shows the detail view with the
// Draw editor; completing the annotation fires annotation-done.
test('mobai mirror: gallery-annotate flow', async ({ page }) => {
  await page.getByRole('button', { name: 'Gallery' }).click();
  const panel = page.getByTestId('gallery-panel');
  await expect(panel).toBeVisible();
  await expect(page.getByTestId('gallery-title')).toContainText('Browser Gallery');

  // Screenshot adds a new demo entry (mock Host capture).
  const before = await panel.getByTestId(/gallery-entry-/).count();
  await page.getByTestId('gallery-screenshot').click();
  await expect(panel.getByTestId(/gallery-entry-/)).toHaveCount(before + 1, { timeout: 5000 });

  // Open the first entry: the detail view appears (faithful, not a hint).
  const entry = panel.getByTestId('gallery-entry-0');
  await expect(entry).toBeVisible();
  await entry.click();
  // Detail view shows the entry name in its toolbar.
  await expect(page.locator('.gallery-detail .gallery-title')).toBeVisible({ timeout: 5000 });

  // Open the Draw editor and complete an annotation.
  await page.getByTestId('gallery-annotate-draw').click();
  // The Draw editor's Done button carries the annotation-done test ID.
  const done = page.getByTestId('annotation-done');
  await expect(done).toBeVisible({ timeout: 5000 });
  await done.click();
  // The demo records the annotation result.
  await expect(page.getByTestId('annotation-result')).toContainText('Annotation done:', { timeout: 5000 });
});

// Mirror of dictation-toggle.mob: dictation control renders with test IDs.
// The Web Speech API is stubbed so the toggle exercises the control plane
// (start/stop) without a mic.
test('mobai mirror: dictation-toggle flow', async ({ page }) => {
  // Stub the Web Speech API before the dictation component mounts.
  await page.addInitScript(() => {
    // The dictation JS calls navigator.mediaDevices.getUserMedia before it
    // constructs the recognizer; headless Chrome has no mic, so stub that
    // too. The fake stream only needs getTracks for the stop path.
    const fakeStream = { getTracks: () => [] };
    if (navigator.mediaDevices) {
      navigator.mediaDevices.getUserMedia = async () => fakeStream;
    } else {
      Object.defineProperty(navigator, 'mediaDevices', {
        value: { getUserMedia: async () => fakeStream },
        configurable: true,
      });
    }
    class FakeRecognition {
      constructor() {
        this.onresult = null;
        this.onend = null;
      }
      start() {
        // Simulate the listening state; no audio needed.
        if (this.onresult) {
          this.onresult({
            results: [[{ transcript: 'demo dictation', isFinal: false }]],
            resultIndex: 0,
          });
        }
      }
      stop() {
        if (this.onend) this.onend();
      }
      abort() {
        if (this.onend) this.onend();
      }
    }
    window.SpeechRecognition = FakeRecognition;
    window.webkitSpeechRecognition = FakeRecognition;
  });

  await page.goto('/');
  await expect(page.locator('.demo-banner')).toContainText('Web component preview', {
    timeout: 30_000,
  });
  await page.getByRole('button', { name: 'Dictation' }).click();

  const wrap = page.getByTestId('dictation-wrap');
  await expect(wrap).toBeVisible();
  const toggle = page.getByTestId('dictation-toggle');
  await expect(toggle).toBeVisible();

  // Start dictation; the status pill appears (listening).
  await toggle.click();
  const status = page.getByTestId('dictation-status');
  await expect(status).toBeVisible({ timeout: 10000 });

  // Stop dictation; the status pill goes away.
  await toggle.click();
  await expect(status).toHaveCount(0, { timeout: 10000 });
});
