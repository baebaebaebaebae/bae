import { defineConfig, devices } from '@playwright/test';
import * as path from 'path';

// The demo web build output directory (relative to bae/)
const DEMO_BUILD_DIR = path.join(__dirname, '../../bae/target/dx/demo_web/release/web/public');
// The covers directory for demo fixtures
const COVERS_DIR = path.join(__dirname, '../fixtures/screenshots/covers');

export default defineConfig({
  testDir: '.',
  fullyParallel: false,
  forbidOnly: !!process.env.CI,
  retries: process.env.CI ? 2 : 0,
  workers: 1,
  reporter: 'list',
  use: {
    baseURL: 'http://localhost:8080',
    trace: 'on-first-retry',
    viewport: { width: 1400, height: 900 },
  },
  projects: [
    {
      name: 'chromium',
      use: { ...devices['Desktop Chrome'] },
    },
  ],
  webServer: {
    // Serve the demo build and mount covers at /covers
    command: `node serve.mjs`,
    url: 'http://localhost:8080',
    reuseExistingServer: !process.env.CI,
    timeout: 120000,
  },
});
