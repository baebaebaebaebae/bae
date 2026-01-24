import { test, expect } from '@playwright/test';
import * as fs from 'fs';
import * as path from 'path';

// Only run screenshot generation in CI
test.skip(() => !process.env.CI, 'Screenshots only run in CI');

const OUTPUT_DIR = path.join(__dirname, '../../website/public/screenshots');

test.beforeAll(async () => {
  // Ensure output directory exists
  fs.mkdirSync(OUTPUT_DIR, { recursive: true });
});

test('capture library view', async ({ page }) => {
  await page.goto('/app');
  
  // Wait for the library to load (albums should be visible)
  await page.waitForSelector('.virtual-grid-content', { timeout: 10000 });
  
  // Give images time to load
  await page.waitForTimeout(500);
  
  await page.screenshot({
    path: path.join(OUTPUT_DIR, 'library.png'),
    fullPage: false,
  });
});

test('capture album detail view', async ({ page }) => {
  await page.goto('/app');
  
  // Wait for albums to load
  await page.waitForSelector('[data-testid="album-card"]', { timeout: 10000 });
  
  // Click on the first album card
  const firstAlbumCard = page.locator('[data-testid="album-card"]').first();
  await firstAlbumCard.click();
  
  // Wait for album detail to load
  await page.waitForSelector('[data-testid="album-detail"]', { timeout: 10000 });
  
  // Give images time to load
  await page.waitForTimeout(500);
  
  await page.screenshot({
    path: path.join(OUTPUT_DIR, 'album-detail.png'),
    fullPage: false,
  });
});
