import { test, expect } from "@playwright/test";
import path from "path";
import { ensureUploadMenuVisible } from "./utils.js";

const fixture = path.resolve(
  process.cwd(),
  "tests",
  "e2e",
  "fixtures",
  "hello.txt",
);

// Basic e2e upload flow
test("upload file and see it listed", async ({ page }) => {
  await page.goto("http://localhost:3000/");

  // open uploader
  await ensureUploadMenuVisible(page);
  await page
    .locator('a.navbar-item:visible:has-text("上传文件")')
    .first()
    .click();
  const uploadModal = page.locator(
    '.modal:has(.modal-card-title:has-text("上传文件"))',
  );
  await uploadModal.waitFor({ state: "visible", timeout: 15000 });

  // wait for file input and set file
  const fileInput = uploadModal.locator('input[type="file"]');
  await fileInput.setInputFiles(fixture);

  // click 上传 button inside uploader
  const uploader = page.locator(".file-uploader");
  await uploader.locator('button:has-text("上传")').click();

  // wait for the file to appear in file list
  await page.waitForSelector('.file-item:has-text("hello.txt")', {
    timeout: 30000,
  });
  await expect(
    page.locator(".file-item", { hasText: "hello.txt" }),
  ).toHaveCount(1);
});
