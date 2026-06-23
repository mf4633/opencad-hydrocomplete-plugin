import { defineConfig } from '@playwright/test';
import { resolveDagWww } from './lib/dag-www.js';

const dagWww = resolveDagWww();
const dagServer = Boolean(dagWww);

export default defineConfig({
  testDir: './specs',
  timeout: 30_000,
  expect: { timeout: 10_000 },
  fullyParallel: true,
  retries: 0,
  reporter: [['list']],
  use: {
    headless: true,
    viewport: { width: 1280, height: 900 },
    actionTimeout: 8_000,
  },
  webServer: dagServer
    ? {
        command: `npx --yes serve "${dagWww!}" -l 4173`,
        url: 'http://127.0.0.1:4173',
        reuseExistingServer: !process.env.CI,
        timeout: 120_000,
      }
    : undefined,
});