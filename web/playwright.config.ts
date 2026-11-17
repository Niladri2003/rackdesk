// End-to-end tests. They run the real server binary with a temporary data
// folder (see e2e/global-setup.ts) and drive the UI in a browser.
//
// Before running: `npm run build` and `cargo build -p rackdeskd`.
// `make e2e` does both.
import { defineConfig, devices } from '@playwright/test'

export const PORT = 7273

export default defineConfig({
  testDir: './e2e',
  globalSetup: './e2e/global-setup.ts',
  // The tests share 1 server and 1 account, and some of them end every
  // session. They run one at a time.
  workers: 1,
  fullyParallel: false,
  forbidOnly: !!process.env.CI,
  retries: process.env.CI ? 1 : 0,
  reporter: process.env.CI ? 'github' : 'list',
  use: {
    baseURL: `http://127.0.0.1:${PORT}`,
    viewport: { width: 1440, height: 900 },
    trace: 'retain-on-failure',
  },
  projects: [{ name: 'chromium', use: { ...devices['Desktop Chrome'], viewport: { width: 1440, height: 900 } } }],
})
