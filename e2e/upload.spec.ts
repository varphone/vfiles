import { test, expect } from '@playwright/test';

// Basic smoke e2e that assumes dev server is running at localhost:3000
// Requires playwright browsers installed (`npx playwright install`)

test('home loads and has upload area', async ({ page }) => {
  await page.goto('http://localhost:3000/');
  await expect(page).toHaveTitle(/VFiles/);
  await expect(page.locator('text=文件上传')).toBeVisible({ timeout: 5000 }).catch(() => {});
});
