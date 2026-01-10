import { test, expect } from '@playwright/test';
import * as fs from 'fs';
import * as path from 'path';

const OUTPUT_DIR = path.join(__dirname, '../../website/public/screenshots');

test.beforeAll(async () => {
  // Ensure output directory exists
  fs.mkdirSync(OUTPUT_DIR, { recursive: true });
});

test('capture library view', async ({ page }) => {
  await page.goto('/');
  
  // Wait for the library to load (albums should be visible)
  await page.waitForSelector('.grid', { timeout: 10000 });
  
  // Give images time to load
  await page.waitForTimeout(500);
  
  await page.screenshot({
    path: path.join(OUTPUT_DIR, 'library.png'),
    fullPage: false,
  });
});

test('capture album detail view', async ({ page }) => {
  await page.goto('/');
  
  // Wait for albums to load
  await page.waitForSelector('.grid', { timeout: 10000 });
  
  // Click on the first album card to navigate to detail view
  const firstAlbumCard = page.locator('.grid > a').first();
  await firstAlbumCard.click();
  
  // Wait for album detail to load
  await page.waitForSelector('h1', { timeout: 10000 });
  
  // Give images time to load
  await page.waitForTimeout(500);
  
  await page.screenshot({
    path: path.join(OUTPUT_DIR, 'album-detail.png'),
    fullPage: false,
  });
});
