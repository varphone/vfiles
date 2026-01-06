/**
 * Ensure the upload menu is open so mobile browsers can see the '上传文件' link.
 * @param {import('@playwright/test').Page} page
 */
export async function ensureUploadMenuVisible(page) {
  const burger = page.locator(".navbar-burger");
  if ((await burger.count()) > 0 && (await burger.first().isVisible())) {
    await burger.first().click();
    await page.waitForTimeout(200);
  }
}
