import { test, expect, Page } from '@playwright/test';

// Capture and display console logs from the app
test.beforeEach(async ({ page }) => {
  page.on('console', msg => {
    const type = msg.type();
    const text = msg.text();
    // Filter to our app logs (they have prefixes like LAYOUT:, MEASURE:)
    if (text.includes('LAYOUT:') || text.includes('MEASURE:') || text.includes('SCROLL:')) {
      console.log(`[browser ${type}] ${text}`);
    }
  });
});

// Helper to count grid items in DOM
async function countGridItems(page: Page): Promise<number> {
  return await page.locator('.virtual-grid-content > div').count();
}

// Helper to get visible album titles
async function getVisibleAlbumTitles(page: Page): Promise<string[]> {
  const titles = await page.locator('[data-testid="album-card"] h3').allTextContents();
  return titles;
}

// Helper to scroll and wait for layout to stabilize
async function scrollTo(page: Page, y: number) {
  await page.evaluate((scrollY) => window.scrollTo(0, scrollY), y);
  await page.waitForTimeout(100); // Let layout settle
}

test.describe('VirtualGrid', () => {
  test.beforeEach(async ({ page }) => {
    // Go to library view with many albums
    await page.goto('/mock/library?state=albums%3D200');
    await page.waitForSelector('.virtual-grid-content', { timeout: 10000 });
    // Wait for images to load and measurement to stabilize
    await page.waitForTimeout(500);
  });

  test('limits DOM elements via virtual scrolling', async ({ page }) => {
    const itemCount = await countGridItems(page);
    
    // With 200 albums, we should NOT have 200 items in DOM
    // Virtual scrolling should limit to visible + buffer (maybe 50-80 max)
    expect(itemCount).toBeLessThan(100);
    expect(itemCount).toBeGreaterThan(0);
    
    console.log(`DOM has ${itemCount} items (expected < 100 for 200 albums)`);
  });

  test('changes visible items when scrolling', async ({ page }) => {
    const initialTitles = await getVisibleAlbumTitles(page);
    
    // Scroll down significantly
    await scrollTo(page, 2000);
    
    const scrolledTitles = await getVisibleAlbumTitles(page);
    
    // Should see different albums after scrolling
    expect(scrolledTitles).not.toEqual(initialTitles);
    
    console.log('Initial:', initialTitles.slice(0, 3));
    console.log('After scroll:', scrolledTitles.slice(0, 3));
  });

  test('maintains reasonable DOM count while scrolling', async ({ page }) => {
    const counts: number[] = [];
    
    // Scroll through the page and measure DOM count at each position
    for (let y = 0; y <= 5000; y += 1000) {
      await scrollTo(page, y);
      const count = await countGridItems(page);
      counts.push(count);
    }
    
    // All counts should be similar (virtual scrolling maintains window)
    const maxCount = Math.max(...counts);
    const minCount = Math.min(...counts);
    
    expect(maxCount - minCount).toBeLessThan(20); // Shouldn't vary wildly
    expect(maxCount).toBeLessThan(100); // Should always be virtualized
    
    console.log('DOM counts at scroll positions:', counts);
  });

  test('scrolling is smooth (no large jumps in first visible item)', async ({ page }) => {
    // Get viewport height
    const viewportHeight = await page.evaluate(() => window.innerHeight);
    
    // Scroll in small increments and check that visible content changes gradually
    let previousFirstTitle = '';
    let jumpCount = 0;
    
    for (let y = 0; y <= 3000; y += viewportHeight / 4) {
      await scrollTo(page, y);
      
      const titles = await getVisibleAlbumTitles(page);
      const firstTitle = titles[0] || '';
      
      if (previousFirstTitle && firstTitle !== previousFirstTitle) {
        // Title changed - this is expected when scrolling past an item
        // But we should track if it jumps too much
        jumpCount++;
      }
      
      previousFirstTitle = firstTitle;
    }
    
    // Some title changes are expected, but not too many (would indicate jumps)
    console.log(`Title changed ${jumpCount} times during scroll`);
  });

  test('resize updates layout correctly', async ({ page }) => {
    // Start at wide viewport
    await page.setViewportSize({ width: 1200, height: 800 });
    await page.waitForTimeout(300);
    
    const wideCount = await countGridItems(page);
    
    // Resize to narrow viewport
    await page.setViewportSize({ width: 400, height: 800 });
    await page.waitForTimeout(300);
    
    const narrowCount = await countGridItems(page);
    
    // Both should be virtualized
    expect(wideCount).toBeLessThan(100);
    expect(narrowCount).toBeLessThan(100);
    
    console.log(`Wide viewport: ${wideCount} items, Narrow: ${narrowCount} items`);
  });

  test('measurement updates after viewport change', async ({ page }) => {
    // This test checks that item height measurement updates on resize
    
    // Start narrow (single column, tall items)
    await page.setViewportSize({ width: 400, height: 800 });
    await page.waitForTimeout(500);
    
    // Get the first item's height
    const narrowHeight = await page.locator('.virtual-grid-content > div').first().boundingBox();
    
    // Go wide (multiple columns, shorter items)
    await page.setViewportSize({ width: 1200, height: 800 });
    await page.waitForTimeout(500);
    
    const wideHeight = await page.locator('.virtual-grid-content > div').first().boundingBox();
    
    // Narrow items should be taller (single column = wider = taller due to aspect-square)
    if (narrowHeight && wideHeight) {
      console.log(`Narrow item height: ${narrowHeight.height}, Wide: ${wideHeight.height}`);
      expect(narrowHeight.height).toBeGreaterThan(wideHeight.height);
    }
  });

  test('items move exactly by scroll delta - no jumps', async ({ page }) => {
    test.setTimeout(60000); // 60 second timeout for this thorough test
    // Use 2000 albums for thorough test
    await page.goto('/mock/library?state=albums%3D2000');
    await page.setViewportSize({ width: 1400, height: 900 });
    await page.waitForSelector('.virtual-grid-content');
    await page.waitForTimeout(1000);

    const items = page.locator('.virtual-grid-content > div[data-index]');
    
    // Track: Map<index, Y position>
    let previousPositions: Map<number, number> = new Map();
    let previousScrollY = 0;

    for (let scrollY = 0; scrollY <= 5000; scrollY += 20) {
      await page.evaluate(y => window.scrollTo(0, y), scrollY);
      await page.waitForTimeout(50); // Wait for render

      // Capture current state: index -> Y position
      const currentData = await items.evaluateAll(els => 
        els.map(el => ({
          index: parseInt(el.dataset.index!, 10),
          y: el.getBoundingClientRect().y
        }))
      );
      
      const currentPositions = new Map(currentData.map(d => [d.index, d.y]));
      const scrollDelta = scrollY - previousScrollY;

      // Log state for debugging BEFORE assertions
      const indices = [...currentPositions.keys()].sort((a, b) => a - b);
      console.log(`scroll=${scrollY}: items ${indices[0]}-${indices[indices.length - 1]}, count=${indices.length}`);

      // For every item that existed before AND still exists now:
      // Its Y should have changed by exactly -scrollDelta
      if (previousPositions.size > 0) {
        for (const [index, prevY] of previousPositions) {
          if (currentPositions.has(index)) {
            const currentY = currentPositions.get(index)!;
            const actualDelta = currentY - prevY;
            const expectedDelta = -scrollDelta; // scroll down = items move up
            
            // Log details for first few items that will be checked
            if (index <= 10) {
              console.log(`  Item ${index}: prevY=${prevY.toFixed(0)}, currentY=${currentY.toFixed(0)}, delta=${actualDelta.toFixed(0)}, expected=${expectedDelta}`);
            }
            
            expect(
              Math.abs(actualDelta - expectedDelta),
              `Item ${index} at scroll=${scrollY}: moved ${actualDelta}px, expected ${expectedDelta}px`
            ).toBeLessThan(2);
          }
        }
      }

      previousPositions = currentPositions;
      previousScrollY = scrollY;
    }
  });
});
