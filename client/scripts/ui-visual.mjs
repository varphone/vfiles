// 像素级截图基线：固定夹具、固定时钟、固定视口/主题/字体，逐页与 goldens 比对。
// 生成/更新：UPDATE_VISUAL=1 bun run ui:visual
// 依赖：playwright + pixelmatch + pngjs；goldens 位于 tests/visual/goldens/。
import { execFileSync, spawn } from "node:child_process";
import {
  mkdtempSync,
  mkdirSync,
  writeFileSync,
  readFileSync,
  existsSync,
  rmSync,
  utimesSync,
} from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { chromium } from "playwright";
import pixelmatch from "pixelmatch";
import { PNG } from "pngjs";

const ROOT = join(dirname(fileURLToPath(import.meta.url)), "..", "..");
const BIN = process.env.VFILES_BIN || resolve(ROOT, "target/debug/vfiles");
const GOLDENS = resolve(ROOT, "client", "tests", "visual", "goldens");
const DIFFS = resolve(ROOT, "client", "tests", "visual", "diffs");
const UPDATE = process.env.UPDATE_VISUAL === "1";
const MAX_DIFF_RATIO = 0.005;
const PIXEL_THRESHOLD = 0.1;
const FIXED_TIME = new Date("2030-01-02T03:04:05Z");
const FIXED_FILE_TIME = new Date("2026-01-02T03:04:05Z");
const THEME_LIGHT = "light";
const THEME_DARK = "dark";
const PORT = 19600 + Math.floor(Math.random() * 200);

let TMP = null;
let server = null;
let browser = null;

function env() {
  return {
    ...process.env,
    VFILES_STORAGE_ROOT: join(TMP, "storage"),
    VFILES_DATABASE_PATH: join(TMP, "vfiles.db"),
    VFILES_AUTH_ENABLED: "true",
    VFILES_AUTH_COOKIE_SECRET: "ui-visual-secret-1234567890abcdefghijkl",
    VFILES_HTTP_PORT: String(PORT),
    VFILES_FTP_ENABLED: "false",
  };
}

function cli(args) {
  return execFileSync(BIN, args, { env: env(), encoding: "utf8" });
}

async function teardown() {
  try {
    await browser?.close();
  } catch {
    /* already closed */
  }
  try {
    server?.kill("SIGTERM");
  } catch {
    /* already stopped */
  }
  if (TMP) rmSync(TMP, { recursive: true, force: true });
}

function seed() {
  TMP = mkdtempSync(join(tmpdir(), "vfiles-ui-visual-"));
  cli(["init"]);
  cli([
    "user", "create",
    "--username", "admin",
    "--email", "admin@example.com",
    "--password", "admin-password-123",
    "--role", "admin",
  ]);
  cli([
    "user", "create",
    "--username", "alice",
    "--email", "alice@example.com",
    "--password", "alice-password-123",
    "--role", "user",
  ]);

  const root = join(TMP, "seed");
  mkdirSync(join(root, "项目库", "设计稿"), { recursive: true });
  const files = [
    ["项目计划.md", "# 项目计划\n\n- 里程碑一\n- 里程碑二\n"],
    ["发布说明.txt", "VFiles 2.7 发布说明\n"],
    ["数据报表.csv", "name,value\nalpha,1\nbeta,2\n"],
    ["组件说明.ts", "export const version = '2.7.0';\n"],
    ["项目库/设计稿/首页.svg", "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"10\" height=\"10\"></svg>\n"],
    ["项目库/需求.md", "# 需求\n\n1. 文件管理\n2. 权限\n"],
  ];
  for (const [name, content] of files) {
    const path = join(root, name);
    mkdirSync(dirname(path), { recursive: true });
    writeFileSync(path, content);
    utimesSync(path, FIXED_FILE_TIME, FIXED_FILE_TIME);
  }
  cli(["import", root, "--flush-files", "20"]);
}

function freezeDatabaseTimestamps() {
  const dbPath = env().VFILES_DATABASE_PATH;
  const script = `
import sqlite3
fixed = "2026-01-02 03:04:05"
con = sqlite3.connect(${JSON.stringify(dbPath)})
for table, columns in {
    "users": ["created_at", "updated_at"],
    "entries": ["created_at", "updated_at"],
    "entry_versions": ["created_at"],
    "user_sessions": ["issued_at", "last_seen_at"],
}.items():
    for column in columns:
        con.execute(f"UPDATE {table} SET {column} = ?", (fixed,))
con.commit()
con.close()
`;
  execFileSync("python3", ["-c", script]);
}

async function waitForServer(base) {
  for (let i = 0; i < 100; i += 1) {
    try {
      const response = await fetch(`${base}/api/health`);
      if (response.ok) return;
    } catch {
      /* not ready */
    }
    await new Promise((done) => setTimeout(done, 200));
  }
  throw new Error("server did not become ready");
}

async function login(page, base) {
  await page.goto(`${base}/login`, { waitUntil: "networkidle" });
  await page.fill("#auth-username", "admin");
  await page.fill("#auth-password", "admin-password-123");
  await page.click(".auth-submit");
  await page.waitForSelector(".file-browser-box", { timeout: 15000 });
  await page.waitForTimeout(500);
}

function visualStyle() {
  return `
    .notifications, .app-bar-bell-badge { display: none !important; }
    *, *::before, *::after { animation-duration: 0s !important; animation-delay: 0s !important; }
  `;
}

async function prepare(page) {
  await page.evaluate(() => document.fonts.ready);
  await page.addStyleTag({ content: visualStyle() });
  await page.evaluate(() => document.activeElement?.blur());
  await page.waitForTimeout(250);
}

function compare(name) {
  const goldenPath = join(GOLDENS, `${name}.png`);
  const actualPath = join(DIFFS, `${name}.actual.png`);
  const diffPath = join(DIFFS, `${name}.diff.png`);
  if (UPDATE) {
    mkdirSync(GOLDENS, { recursive: true });
    writeFileSync(goldenPath, readFileSync(actualPath));
    return { name, diff: 0, ratio: 0, updated: true };
  }
  if (!existsSync(goldenPath)) {
    throw new Error(`缺少 golden：${goldenPath}（首次运行请用 UPDATE_VISUAL=1）`);
  }
  const golden = PNG.sync.read(readFileSync(goldenPath));
  const actual = PNG.sync.read(readFileSync(actualPath));
  if (golden.width !== actual.width || golden.height !== actual.height) {
    throw new Error(
      `${name}: 尺寸 ${actual.width}x${actual.height} != ${golden.width}x${golden.height}`,
    );
  }
  const diff = new PNG({ width: golden.width, height: golden.height });
  const diffPixels = pixelmatch(
    golden.data,
    actual.data,
    diff.data,
    golden.width,
    golden.height,
    { threshold: PIXEL_THRESHOLD, includeAA: false },
  );
  const ratio = diffPixels / (golden.width * golden.height);
  if (diffPixels > 0) writeFileSync(diffPath, PNG.sync.write(diff));
  return { name, diff: diffPixels, ratio, updated: false };
}

async function shot(browser, base, config) {
  const context = await browser.newContext({
    viewport: config.viewport ?? { width: 1440, height: 900 },
    isMobile: config.mobile ?? false,
    hasTouch: config.mobile ?? false,
  });
  const page = await context.newPage();
  await page.clock.setFixedTime(FIXED_TIME);
  await page.addInitScript(
    (theme) => localStorage.setItem("vfiles:theme", theme),
    config.theme ?? THEME_LIGHT,
  );
  if (config.loginPage) {
    await page.goto(`${base}/login`, { waitUntil: "networkidle" });
  } else {
    await login(page, base);
  }
  if (config.setup) await config.setup(page);
  await prepare(page);
  const buffer = await page.screenshot({
    animations: "disabled",
    caret: "hide",
    scale: "css",
    fullPage: false,
  });
  mkdirSync(DIFFS, { recursive: true });
  writeFileSync(join(DIFFS, `${config.name}.actual.png`), buffer);
  await context.close();
  return config.name;
}

async function main() {
  if (!existsSync(BIN)) throw new Error(`缺少 ${BIN}；先构建服务端`);
  if (!existsSync(join(ROOT, "client", "dist", "index.html"))) {
    throw new Error("缺少 client/dist；先运行 bun run build");
  }
  seed();
  freezeDatabaseTimestamps();
  server = spawn(BIN, ["serve"], { env: env(), stdio: "ignore", cwd: ROOT });
  const base = `http://127.0.0.1:${PORT}`;
  await waitForServer(base);

  browser = await chromium.launch();
  const shots = [
    // 登录页无需鉴权，单独作为首屏基线
    {
      name: "login-light",
      loginPage: true,
      setup: async (page) => {
        await page.waitForSelector(".auth-submit");
      },
    },
    {
      name: "files-light",
      setup: async (page) => {
        await page.waitForSelector("tr.desktop-file-row");
      },
    },
    {
      name: "files-dark",
      theme: THEME_DARK,
      setup: async (page) => {
        await page.waitForSelector("tr.desktop-file-row");
      },
    },
    {
      name: "grid-light",
      setup: async (page) => {
        await page.locator('button[aria-label="网格视图"]').click();
        await page.waitForSelector(".file-card");
      },
    },
    {
      name: "mobile-files-light",
      viewport: { width: 390, height: 844 },
      mobile: true,
      setup: async (page) => {
        await page.waitForSelector(".file-item");
      },
    },
    {
      name: "mobile-files-dark",
      viewport: { width: 390, height: 844 },
      mobile: true,
      theme: THEME_DARK,
      setup: async (page) => {
        await page.waitForSelector(".file-item");
      },
    },
    {
      name: "admin-users-light",
      setup: async (page) => {
        await page.goto(`${base}/admin/users`, { waitUntil: "networkidle" });
        await page.waitForSelector("table tbody tr");
      },
    },
    {
      name: "tokens-empty-light",
      setup: async (page) => {
        await page.goto(`${base}/settings/tokens`, { waitUntil: "networkidle" });
        await page.waitForSelector(".settings-content-panel");
      },
    },
    {
      name: "search-light",
      setup: async (page) => {
        await page.keyboard.press("Control+k");
        await page.waitForSelector(".modal.is-active .unified-search");
        await page.fill(".unified-search-input", "项目");
        await page.waitForTimeout(800);
        await page.waitForSelector(".unified-search-result");
      },
    },
    {
      name: "context-menu-light",
      setup: async (page) => {
        await page.locator("tr.desktop-file-row").first().click({ button: "right" });
        await page.waitForSelector(".vfiles-context-menu");
      },
    },
  ];

  const results = [];
  for (const config of shots) {
    const name = await shot(browser, base, config);
    const result = compare(name);
    results.push(result);
    const mark = result.diff === 0 ? "✓" : "✗";
    console.log(
      `[visual] ${mark} ${name}${result.updated ? "（已写入基线）" : ` — diff ${result.diff} (${(result.ratio * 100).toFixed(3)}%)`}`,
    );
  }
  await browser.close();

  const failed = results.filter(
    (result) => !result.updated && result.ratio > MAX_DIFF_RATIO,
  );
  console.log(
    `\n[visual] ${results.length - failed.length}/${results.length} within ${(MAX_DIFF_RATIO * 100).toFixed(1)}%`,
  );
  if (failed.length) process.exitCode = 1;
}

main()
  .catch((error) => {
    console.error("[visual] 失败：", error);
    process.exitCode = 1;
  })
  .finally(teardown);
