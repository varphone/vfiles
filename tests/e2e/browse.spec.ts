import { test, expect } from "@playwright/test";
import path from "path";
import { ensureUploadMenuVisible } from "./utils.js";

test("browse and search for file", async ({ page }) => {
  await page.goto("http://localhost:3000/");

  // ensure repo is clean before uploading the fixture (remove leftover from prior run)
  await page.request
    .delete("http://localhost:3000/api/files?path=hello.txt")
    .catch(() => {});

  // ensure file exists by uploading via UI
  await ensureUploadMenuVisible(page);
  await page
    .locator('a.navbar-item:visible:has-text("上传文件")')
    .first()
    .click();
  const uploadModal = page.locator(
    '.modal:has(.modal-card-title:has-text("上传文件"))',
  );
  await uploadModal.waitFor({ state: "visible", timeout: 15000 });
  const fixture = path.resolve(
    process.cwd(),
    "tests",
    "e2e",
    "fixtures",
    "hello.txt",
  );
  await page.locator('input[type="file"]').setInputFiles(fixture);
  const uploader = page.locator(".file-uploader");
  await uploader.locator('button:has-text("上传")').click();

  // wait for upload modal to close (indicates successful upload)
  await uploadModal.waitFor({ state: "hidden", timeout: 30000 });

  // wait for file to be present
  await page.waitForSelector('.file-item:has-text("hello.txt")', {
    timeout: 30000,
  });

  // search using the search input
  await page
    .fill('input[placeholder="搜索文件名..."]', "hello.txt")
    .catch(() => {});
  await page.keyboard.press("Enter");

  // expect search results show the file
  await page.waitForSelector('.file-item:has-text("hello.txt")', {
    timeout: 10000,
  });
  await expect(
    page.locator(".file-item", { hasText: "hello.txt" }),
  ).toHaveCount(1);

  // clean up after verifying search so subsequent tests start from a clean repo
  await page.request
    .delete("http://localhost:3000/api/files?path=hello.txt")
    .catch(() => {});
});
