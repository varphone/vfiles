import { test, expect } from "@playwright/test";
import path from "path";
import { readFile } from "node:fs/promises";
import { ensureUploadMenuVisible } from "./utils.js";

const f1 = path.resolve(process.cwd(), "tests", "e2e", "fixtures", "hello.txt");
const f2 = path.resolve(
  process.cwd(),
  "tests",
  "e2e",
  "fixtures",
  "hello2.txt",
);
const historyUrl = "http://localhost:3000/api/history?path=hello.txt";

test("upload twice and check history via UI", async ({ page }) => {
  test.setTimeout(60000);
  page.on("response", (resp) => {
    const url = resp.request().url();
    if (url.includes("/files/upload")) {
      console.log("resp observed", url, resp.request().method());
    }
  });
  await page.goto("http://localhost:3000/");

  const uploadModal = page.locator(
    '.modal:has(.modal-card-title:has-text("上传文件"))',
  );

  async function fetchHistoryCommits(): Promise<any[]> {
    try {
      const res = await page.request.get(`${historyUrl}&_=${Date.now()}`, {
        timeout: 5000,
        headers: {
          "cache-control": "no-cache, no-store",
          pragma: "no-cache",
        },
      });
      if (!res.ok()) return [];
      const body = await res.json();
      return body.data?.commits || [];
    } catch {
      return [];
    }
  }

  async function waitForNewCommit(seen: Set<string>): Promise<string | null> {
    for (let i = 0; i < 60; i++) {
      const commits = await fetchHistoryCommits();
      console.log(
        "history poll",
        commits.map((c: any) => c.hash),
      );
      const candidate = commits.find((c: any) => c.hash && !seen.has(c.hash));
      if (candidate?.hash) {
        seen.add(candidate.hash);
        return candidate.hash;
      }
      await page.waitForTimeout(1000);
    }
    return null;
  }

  const baselineCommits = await fetchHistoryCommits();
  const seenHashes = new Set<string>(
    baselineCommits.map((c: any) => c.hash).filter(Boolean),
  );
  console.log("history baseline", Array.from(seenHashes));

  // upload first version
  await ensureUploadMenuVisible(page);
  await page
    .locator('a.navbar-item:visible:has-text("上传文件")')
    .first()
    .click();
  await uploadModal.waitFor({ state: "visible", timeout: 15000 });
  await uploadModal.locator('input[type="file"]').setInputFiles(f1);
  const uploader = uploadModal.locator(".file-uploader");
  const firstUploadResponse = page.waitForResponse(
    (resp) =>
      resp.request().url().includes("/files/upload") &&
      resp.request().method() === "POST" &&
      resp.ok(),
    { timeout: 30000 },
  );
  await uploader.locator('button:has-text("上传")').click();
  await firstUploadResponse;
  await page.waitForSelector("text=hello.txt", { timeout: 15000 });
  const commit1 = await waitForNewCommit(seenHashes);
  expect(commit1).toBeTruthy();
  console.log("history after first upload", commit1);

  // reload page to ensure modal/overlay cleared, then upload second version via UI
  await page.reload();
  await ensureUploadMenuVisible(page);
  await page
    .locator('a.navbar-item:visible:has-text("上传文件")')
    .first()
    .click();
  await uploadModal.waitFor({ state: "visible", timeout: 15000 });
  const alternativeContent = await readFile(f2);
  await uploadModal.locator('input[type="file"]').setInputFiles({
    name: "hello.txt",
    mimeType: "text/plain",
    buffer: alternativeContent,
  });
  const uploader2 = uploadModal.locator(".file-uploader");
  const secondUploadResponse = page.waitForResponse(
    (resp) =>
      resp.request().url().includes("/files/upload") &&
      resp.request().method() === "POST" &&
      resp.ok(),
    { timeout: 30000 },
  );
  await uploader2.locator('button:has-text("上传")').click();
  await secondUploadResponse;
  await page.waitForSelector("text=hello.txt", { timeout: 15000 });
  const commit2 = await waitForNewCommit(seenHashes);
  expect(commit2).toBeTruthy();
  console.log("history e2e commits", { commit1, commit2 });

  await page.reload();
  await page.waitForLoadState("networkidle");
  const row = page.locator(".file-item", { hasText: "hello.txt" }).first();
  await row.waitFor({ state: "visible", timeout: 60000 });
  await row.scrollIntoViewIfNeeded();
  await row.locator('button[title="查看历史"]').click();

  // wait for at least 2 commits to appear in the history UI
  const commitsUi = page.locator(".version-history .timeline-item");
  await commitsUi.nth(1).waitFor({ timeout: 60000 });
  const count = await commitsUi.count();
  expect(count).toBeGreaterThanOrEqual(2);
});
