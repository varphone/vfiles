// 浏览器级 UI 回归：键盘可用性、对话框/URL 行为，以及 axe 可访问性扫描。
//
// 自带夹具生命周期（临时存储 + 显式 VFILES_* 环境 + 退出清理），不触碰仓库 data/。
// 依赖：client 的 playwright + @axe-core/playwright；服务器二进制默认
// target/debug/vfiles（可用 VFILES_BIN 覆盖），前端取 client/dist。
//
// 用法：bun run ui:regression
//       失败时打印逐项结果并非零退出；成功时打印摘要。

import { execFileSync, spawn } from "node:child_process";
import { mkdtempSync, mkdirSync, writeFileSync, existsSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { chromium } from "playwright";
import AxeBuilder from "@axe-core/playwright";

const ROOT = join(dirname(fileURLToPath(import.meta.url)), "..", "..");
const BIN = process.env.VFILES_BIN || resolve(ROOT, "target/debug/vfiles");
const PORT = 19400 + Math.floor(Math.random() * 200);
const THEME = "light";
const USER = { name: "admin", pass: "admin-password-123" };

const results = [];
async function waitFor(predicate, timeout = 5000) {
  const deadline = Date.now() + timeout;
  while (Date.now() < deadline) {
    try {
      if (await predicate()) return true;
    } catch {
      /* 页面正在切换 */
    }
    await new Promise((done) => setTimeout(done, 100));
  }
  return false;
}

function check(name, ok, detail = "") {
  results.push({ name, ok: Boolean(ok), detail });
  const mark = ok ? "✓" : "✗";
  console.log(`[ui] ${mark} ${name}${detail ? ` — ${detail}` : ""}`);
}

let TMP = null;
let server = null;

function env(extra = {}) {
  return {
    ...process.env,
    VFILES_STORAGE_ROOT: join(TMP, "storage"),
    VFILES_DATABASE_PATH: join(TMP, "vfiles.db"),
    VFILES_AUTH_ENABLED: "true",
    VFILES_AUTH_COOKIE_SECRET: "ui-regression-secret-1234567890abcdefghijkl",
    VFILES_HTTP_PORT: String(PORT),
    VFILES_FTP_ENABLED: "false",
    ...extra,
  };
}

function cli(args) {
  return execFileSync(BIN, args, { env: env(), encoding: "utf8" });
}

function teardown() {
  try {
    server?.kill("SIGTERM");
  } catch {
    /* 已退出 */
  }
  if (TMP) rmSync(TMP, { recursive: true, force: true });
}

async function main() {
  if (!existsSync(BIN)) {
    throw new Error(`缺少 ${BIN}；先运行 cargo build -p vfiles-bin`);
  }
  if (!existsSync(join(ROOT, "client", "dist", "index.html"))) {
    throw new Error("缺少 client/dist；先运行 bun run build");
  }

  TMP = mkdtempSync(join(tmpdir(), "vfiles-ui-regression-"));
  cli(["init"]);
  cli([
    "user", "create",
    "--username", USER.name,
    "--email", "admin@example.com",
    "--password", USER.pass,
    "--role", "admin",
  ]);
  cli([
    "user", "create",
    "--username", "alice",
    "--email", "alice@example.com",
    "--password", "alice-password-123",
    "--role", "user",
  ]);

  const seed = join(TMP, "seed");
  mkdirSync(join(seed, "项目库", "子目录"), { recursive: true });
  writeFileSync(join(seed, "项目库", "说明.md"), "# 说明\n");
  writeFileSync(join(seed, "说明.md"), "# 说明\n");
  for (let i = 1; i <= 24; i += 1) {
    writeFileSync(
      join(seed, `报告-${String(i).padStart(2, "0")}.md`),
      `# ${i}\n`,
    );
  }
  cli(["import", seed, "--flush-files", "30"]);

  server = spawn(BIN, ["serve"], { env: env(), stdio: "ignore", cwd: ROOT });
  const base = `http://127.0.0.1:${PORT}`;
  for (let i = 0; i < 100; i += 1) {
    try {
      const response = await fetch(`${base}/api/health`);
      if (response.ok) break;
    } catch {
      /* 未就绪 */
    }
    await new Promise((done) => setTimeout(done, 200));
    if (i === 99) throw new Error("服务器未就绪");
  }

  const browser = await chromium.launch();
  const context = await browser.newContext({
    viewport: { width: 1440, height: 900 },
  });
  const page = await context.newPage();
  const pageErrors = [];
  page.on("pageerror", (error) => pageErrors.push(String(error)));
  page.on("console", (message) => {
    if (message.type() === "error") pageErrors.push(message.text());
  });
  await page.addInitScript(
    (theme) => localStorage.setItem("vfiles:theme", theme),
    THEME,
  );

  await page.goto(`${base}/login`, { waitUntil: "networkidle" });
  await page.fill("#auth-username", USER.name);
  await page.fill("#auth-password", USER.pass);
  await page.click(".auth-submit");
  await page.waitForSelector(".file-browser-box", { timeout: 15000 });
  await page.waitForTimeout(700);

  // 1) 控件上的 Enter/Space 不被全局文件快捷键吞掉。
  await page.locator(".nc-new-button").focus();
  await page.keyboard.press("Enter");
  check(
    "Enter on +new opens its menu",
    await waitFor(async () => (await page.locator(".nc-new-dropdown").count()) === 1),
  );
  await page.keyboard.press("Escape");
  await page.waitForTimeout(200);

  await page.locator('.views-sidebar-item[href="/recent"]').focus();
  await page.keyboard.press("Enter");
  check(
    "Enter on a sidebar link navigates",
    await waitFor(() => new URL(page.url()).pathname === "/recent"),
    page.url(),
  );

  await page.goto(`${base}/`, { waitUntil: "networkidle" });
  await page.waitForSelector(".file-browser-box");
  await page.waitForTimeout(500);
  await page.locator(".app-header-search").focus();
  await page.keyboard.press(" ");
  check(
    "Space on the search button opens search",
    await waitFor(
      async () =>
        (await page.locator(".modal.is-active .unified-search").count()) === 1,
    ),
  );
  await page.keyboard.press("Escape");
  await page.waitForTimeout(300);

  // 2) 光标行不再伪装成选中。
  const rowState = await page.evaluate(() => {
    const row = document.querySelector("tr.desktop-file-row");
    return {
      active: row?.classList.contains("is-row-active") ?? false,
      selected: row?.getAttribute("aria-selected") ?? null,
      checked: row?.querySelector('input[type="checkbox"]')?.checked ?? null,
      selectedRows: document.querySelectorAll(
        "tr.desktop-file-row.is-row-selected",
      ).length,
    };
  });
  check(
    "cursor row is active but not selected",
    rowState.active && rowState.selected === null && rowState.checked === false,
    JSON.stringify(rowState),
  );
  check("no row is selected on load", rowState.selectedRows === 0);

  // 3) 右键菜单键盘操作与焦点归还。
  await page.click('tr[data-vfiles-path="说明.md"]');
  await page.keyboard.press("Shift+F10");
  const menuFocused = await waitFor(async () => {
    const state = await page.evaluate(() => ({
      open: document.querySelectorAll(".vfiles-context-menu").length,
      role: document.activeElement?.getAttribute("role"),
    }));
    return state.open === 1 && state.role === "menuitem";
  });
  check(
    "Shift+F10 focuses the context menu",
    menuFocused,
    JSON.stringify(
      await page.evaluate(() => ({
        open: document.querySelectorAll(".vfiles-context-menu").length,
        role: document.activeElement?.getAttribute("role"),
      })),
    ),
  );
  const beforeArrow = await page.evaluate(
    () => document.activeElement?.textContent?.trim() ?? "",
  );
  await page.keyboard.press("ArrowDown");
  check(
    "arrow keys move inside the menu",
    await waitFor(
      async () =>
        (await page.evaluate(
          () => document.activeElement?.textContent?.trim() ?? "",
        )) !== beforeArrow,
    ),
  );
  await page.keyboard.press("Escape");
  check(
    "Escape closes the menu and returns to the row menu",
    await waitFor(async () => {
      const gone = (await page.locator(".vfiles-context-menu").count()) === 0;
      const onRowMenu = await page.evaluate(() =>
        Boolean(document.activeElement?.classList.contains("desktop-row-menu")),
      );
      return gone && onRowMenu;
    }),
  );

  // 4) 对话框：滚动锁、背景 inert、焦点归还。
  await page.click(".nc-new-button");
  await page.waitForSelector(".nc-new-dropdown");
  await page.click('.nc-new-dropdown .nc-new-item:has-text("新建文件夹")');
  await page.waitForSelector(".modal.is-active .modal-card");
  const dialogLocked = await waitFor(async () => {
    const state = await page.evaluate(() => ({
      bodyOverflow: getComputedStyle(document.body).overflow,
      inert: document.querySelectorAll("[inert]").length,
      focusInside: Boolean(document.activeElement?.closest(".modal-card")),
    }));
    return (
      state.bodyOverflow === "hidden" && state.inert > 0 && state.focusInside
    );
  });
  check(
    "dialog locks scroll and inerts the background",
    dialogLocked,
    JSON.stringify(
      await page.evaluate(() => ({
        bodyOverflow: getComputedStyle(document.body).overflow,
        inert: document.querySelectorAll("[inert]").length,
        focusInside: Boolean(document.activeElement?.closest(".modal-card")),
      })),
    ),
  );
  await page.keyboard.press("Escape");
  check(
    "dialog returns focus to its opener",
    await waitFor(() =>
      page.evaluate(() =>
        Boolean(document.activeElement?.classList.contains("nc-new-button")),
      ),
    ),
  );

  // 5) 目录写入 URL，支持后退/刷新。
  await page.evaluate(() =>
    document
      .querySelector('tr[data-vfiles-path="项目库"]')
      ?.dispatchEvent(new MouseEvent("dblclick", { bubbles: true })),
  );
  check(
    "opening a folder writes ?path=",
    await waitFor(
      () => new URL(page.url()).searchParams.get("path") === "项目库",
    ),
    page.url(),
  );
  await page.goBack();
  check(
    "browser Back returns to the parent directory",
    await waitFor(async () => {
      const noPath = !new URL(page.url()).searchParams.has("path");
      const row = (await page.locator('tr[data-vfiles-path="项目库"]').count()) === 1;
      return noPath && row;
    }),
    page.url(),
  );

  // 6) axe：主要页面 serious/critical 违规为 0。
  const pages = [
    ["/", ".file-browser-box"],
    ["/recent", ".recent-view"],
    ["/favorites", ".favorites-view"],
    ["/activity", ".activity-view"],
    ["/settings/tokens", "section"],
    ["/shares", "section"],
    ["/admin", "main"],
    ["/admin/users", "table"],
    ["/admin/audit", "table"],
    ["/admin/system", "section"],
  ];
  const axeFailures = [];
  for (const [path, selector] of pages) {
    await page.goto(`${base}${path}`, { waitUntil: "networkidle" });
    try {
      await page.waitForSelector(selector, { timeout: 8000 });
    } catch {
      /* 页面可能用空态渲染 */
    }
    await page.waitForTimeout(350);
    const report = await new AxeBuilder({ page })
      .withTags(["wcag2a", "wcag2aa", "wcag21a", "wcag21aa", "wcag22aa"])
      .analyze();
    const blocked = report.violations.filter(
      (violation) =>
        violation.impact === "critical" || violation.impact === "serious",
    );
    for (const violation of blocked) {
      axeFailures.push({
        path,
        id: violation.id,
        impact: violation.impact,
        nodes: violation.nodes.length,
        targets: violation.nodes.map((node) => node.target),
        html: violation.nodes.map((node) => node.html),
        summary: violation.nodes.map((node) => node.failureSummary),
      });
    }
  }
  check(
    "axe serious/critical violations",
    axeFailures.length === 0,
    axeFailures.length ? JSON.stringify(axeFailures, null, 2) : "0",
  );

  // 7) 运行期间没有未捕获异常/控制台错误。
  check("no page errors", pageErrors.length === 0, pageErrors.slice(0, 3).join(" | "));

  await browser.close();

  const failed = results.filter((result) => !result.ok);
  console.log(
    `\n[ui] ${results.length - failed.length}/${results.length} checks passed`,
  );
  if (failed.length > 0) {
    process.exitCode = 1;
  }
}

main()
  .catch((error) => {
    console.error("[ui] 失败：", error);
    process.exitCode = 1;
  })
  .finally(teardown);
