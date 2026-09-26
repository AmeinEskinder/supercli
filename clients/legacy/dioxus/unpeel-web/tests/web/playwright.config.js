// @ts-check
const { defineConfig } = require('@playwright/test');

module.exports = defineConfig({
  testDir: '.',
  testMatch: ['*.spec.js'],
  timeout: 60_000,
  webServer: {
    command: 'node server.js',
    port: 8327,
    reuseExistingServer: true,
    stdout: 'pipe',
  },
  use: {
    baseURL: 'http://127.0.0.1:8327',
    // PLAYWRIGHT_CHROMIUM_PATH points at a Chromium binary when the
    // Playwright-managed browser download is unavailable (proxied CI, etc.).
    // CI installs the managed browser and leaves this unset.
    launchOptions: process.env.PLAYWRIGHT_CHROMIUM_PATH
      ? { executablePath: process.env.PLAYWRIGHT_CHROMIUM_PATH }
      : {},
  },
});
