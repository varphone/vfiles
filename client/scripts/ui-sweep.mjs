// UI 截图回归：14 个表面 × 双主题 → PNG 证据集（bun run ui:sweep）。
//
// 自带完整夹具生命周期（round 23 事故的纪律内建）：
//   mktemp 存储库 + 显式 VFILES_* 环境的 CLI/服务器 + 结束清理 —— 全程不触碰仓库 data/。
//
// 用法：
//   bun run ui:sweep                 # 输出到 /tmp/vfiles-ui-sweep-<时间戳>/
//   bun run ui:sweep -- --out DIR    # 指定输出目录
//   bun run ui:sweep -- --keep       # 保留临时存储与服务器（调试用）
// 依赖 playwright（未随 client 安装时会回退解析 /tmp/vf-shot 的安装并给出安装提示）。

import { execFileSync, spawn } from "node:child_process";
import {
  mkdtempSync,
  mkdirSync,
  writeFileSync,
  existsSync,
  rmSync,
} from "node:fs";
import { tmpdir } from "node:os";
import { join, dirname, resolve } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";

const ROOT = join(dirname(fileURLToPath(import.meta.url)), "..", "..");
const VFILES_BIN = resolve(ROOT, "target/debug/vfiles");

const args = process.argv.slice(2);
const argVal = (flag) => {
  const i = args.indexOf(flag);
  return i >= 0 ? args[i + 1] : undefined;
};
if (args.includes("--help")) {
  console.log("用法: bun run ui:sweep [-- --out DIR] [--keep]");
  process.exit(0);
}
const OUT_DIR =
  argVal("--out") || join(tmpdir(), `vfiles-ui-sweep-${Date.now()}`);
const KEEP = args.includes("--keep");
const PORT = 19500 + Math.floor(Math.random() * 300);
const SEED_USER = { name: "admin", pass: "admin-password-123" };

async function loadPlaywright() {
  for (const from of ["playwright", "/tmp/vf-shot/node_modules/playwright"]) {
    try {
      return await import(from.startsWith("/") ? pathToFileURL(from) : from);
    } catch {
      /* 继续回退 */
    }
  }
  console.error(
    "需要 playwright：bun add -d playwright（或在 /tmp/vf-shot 已有安装）",
  );
  process.exit(1);
}

const log = (msg) => console.log(`[ui-sweep] ${msg}`);
let TMP = null;
let server = null;

function env(extra = {}) {
  return {
    ...process.env,
    VFILES_STORAGE_ROOT: join(TMP, "storage"),
    VFILES_DATABASE_PATH: join(TMP, "vfiles.db"),
    VFILES_AUTH_ENABLED: "true",
    VFILES_AUTH_COOKIE_SECRET: "ui-sweep-secret-ui-sweep-secret-1234567890ab",
    VFILES_HTTP_PORT: String(PORT),
    VFILES_FTP_ENABLED: "false",
    ...extra,
  };
}

function vfiles(cmd) {
  return execFileSync(VFILES_BIN, cmd, { env: env(), encoding: "utf8" });
}

function teardown() {
  try {
    server?.kill("SIGTERM");
  } catch {
    /* 已退出 */
  }
  if (!KEEP && TMP) rmSync(TMP, { recursive: true, force: true });
  log(KEEP ? `已保留临时环境：${TMP}` : "临时环境已清理");
}

async function main() {
  if (!existsSync(VFILES_BIN)) {
    console.error(`缺少 ${VFILES_BIN}，请先 cargo build -p vfiles-bin`);
    process.exit(1);
  }
  TMP = mkdtempSync(join(tmpdir(), "vfiles-ui-sweep-"));
  mkdirSync(OUT_DIR, { recursive: true });
  log(`临时存储 ${TMP}（显式环境，绝不触碰仓库 data/）`);
  log(`输出目录 ${OUT_DIR}`);

  // ---- 夹具（全部 CLI 走显式 env） ----
  vfiles(["init"]);
  vfiles([
    "user",
    "create",
    "--username",
    SEED_USER.name,
    "--email",
    "admin@example.com",
    "--password",
    SEED_USER.pass,
    "--role",
    "admin",
  ]);
  vfiles([
    "user",
    "create",
    "--username",
    "alice",
    "--email",
    "alice@example.com",
    "--password",
    "alice-password-123",
    "--role",
    "user",
  ]);
  const seed = join(TMP, "seed");
  mkdirSync(join(seed, "项目库", "子目录"), { recursive: true });
  writeFileSync(
    join(seed, "项目库", "说明.md"),
    "# 说明文档\n\n**加粗** 与 `代码`。\n",
  );
  writeFileSync(
    join(seed, "项目库", "代码.js"),
    '// 注释 comment\nfunction greet(name) {\n  const msg = "hello";\n  return msg + name;\n}\nconst n = 42;\n',
  );
  writeFileSync(join(seed, "项目库", "子目录", "深层.txt"), "x\n");
  writeFileSync(join(seed, "文档.txt"), "v1\n");
  vfiles(["import", seed, "--flush-files", "50"]);
  log("夹具就绪（项目库/文档.txt/代码.js 等）");

  // ---- 服务器（同 env） ----
  server = spawn(VFILES_BIN, ["serve"], { env: env(), stdio: "ignore" });
  const base = `http://127.0.0.1:${PORT}`;
  for (let i = 0; i < 100; i++) {
    try {
      const r = await fetch(`${base}/api/health`);
      if (r.ok) break;
    } catch {
      /* 未就绪 */
    }
    await new Promise((r) => setTimeout(r, 250));
    if (i === 99) throw new Error("服务器未就绪");
  }
  log(`服务器就绪 ${base}`);

  // ---- 截图（含历轮教训：铃铛在主文件页截、重命名用唯一名） ----
  const { chromium, devices } = await loadPlaywright();
  const browser = await chromium.launch();
  const shots = [];
  // 程序化 dblclick：物理双击曾因选中时复选框插入位移而第二击落空（已修行内位移，
  // 但工具层面仍用事件派发求稳——应用处理器对合成事件与物理事件等价响应 ✓ 实测）。
  const doubleClick = async (locator) => {
    await locator.evaluate((el) =>
      el.dispatchEvent(new MouseEvent("dblclick", { bubbles: true })),
    );
  };
  const snap = async (page, name) => {
    const path = join(OUT_DIR, `${name}.png`);
    await page.screenshot({ path });
    shots.push(`${name}.png`);
    log(`截取 ${name}`);
  };

  for (const theme of ["light", "dark"]) {
    // 桌面
    const page = await (
      await browser.newContext({ viewport: { width: 1440, height: 900 } })
    ).newPage();
    await page.addInitScript(
      (t) => localStorage.setItem("vfiles:theme", t),
      theme,
    );
    await page.goto(`${base}/login`, { waitUntil: "networkidle" });
    await page.waitForTimeout(400);
    await snap(page, `${theme}-1-login`);
    await page.fill("#auth-username", SEED_USER.name);
    await page.fill("#auth-password", SEED_USER.pass);
    await page.click(".auth-submit");
    await page.waitForSelector(".file-browser-box", { timeout: 10000 });
    await page.waitForTimeout(900);
    await snap(page, `${theme}-2-list`);
    await doubleClick(page.locator('tr[data-vfiles-path="项目库"]'));
    await page.waitForTimeout(700);
    await page.evaluate(() =>
      document
        .querySelector("tr.desktop-file-row input[type=checkbox]")
        ?.click(),
    );
    await page.waitForTimeout(500);
    await snap(page, `${theme}-3-batch`);
    await page
      .locator("tr.desktop-file-row")
      .first()
      .click({ button: "right" });
    await page.waitForSelector(".vfiles-context-menu");
    await page.waitForTimeout(300);
    await snap(page, `${theme}-4-ctxmenu`);
    await page.keyboard.press("Escape");
    await page.mouse.click(700, 620);
    await page.waitForTimeout(300);
    await doubleClick(page.locator('tr[data-vfiles-path="项目库/代码.js"]'));
    await page.waitForTimeout(1200);
    await snap(page, `${theme}-5-preview-code`);
    await page.keyboard.press("Escape");
    await page.waitForTimeout(400);
    // 历史（唯一命名的两次改名，绝不撞名）
    // 成对路径（from → to）：每次改名后路径即变，固定路径会在第二击落空；
    // 脆弱步骤 try/catch 隔离：单步失败不再拖垮整个 sweep。
    for (const [from, to] of [
      ["项目库/说明.md", "唯一A.tmp"],
      ["项目库/唯一A.tmp", "文档-副本.txt"],
    ]) {
      try {
        await page
          .locator(`tr[data-vfiles-path="${from}"]`)
          .click({ button: "right" });
        await page.waitForSelector(".vfiles-context-menu");
        await page.click(
          '.vfiles-context-menu [role="menuitem"]:has-text("重命名")',
        );
        await page.waitForSelector(".rename-input", { timeout: 4000 });
        await page.fill(".rename-input", to);
        await page.keyboard.press("Enter");
        await page.waitForTimeout(900);
      } catch (e) {
        console.log(`[ui-sweep] ✗ 改名(${from}): ${String(e).slice(0, 80)}`);
      }
    }
    const hb = await page.$('button:has-text("历史版本")');
    if (hb) {
      await hb.click();
      await page.waitForTimeout(1500);
    }
    await snap(page, `${theme}-6-history`);
    await page.keyboard.press("Escape");
    await page.waitForTimeout(400);
    await page.click(".desktop-command-bar .view-options button");
    await page.waitForTimeout(450);
    await snap(page, `${theme}-7-viewpanel`);
    await page.click('.view-options-panel button:has-text("网格")');
    await page.waitForTimeout(700);
    await snap(page, `${theme}-8-grid`);
    await page.close();

    // 工具页（无铃铛）+ 通知（回主文件页）
    const p2 = await (
      await browser.newContext({ viewport: { width: 1440, height: 900 } })
    ).newPage();
    await p2.addInitScript(
      (t) => localStorage.setItem("vfiles:theme", t),
      theme,
    );
    await p2.goto(`${base}/login`, { waitUntil: "networkidle" });
    await p2.fill("#auth-username", SEED_USER.name);
    await p2.fill("#auth-password", SEED_USER.pass);
    await p2.click(".auth-submit");
    await p2.waitForSelector(".file-browser-box", { timeout: 10000 });
    await p2.waitForTimeout(800);
    await p2.goto(`${base}/settings/tokens`, { waitUntil: "networkidle" });
    await p2.waitForTimeout(800);
    await snap(p2, `${theme}-9-tokens`);
    await p2.goto(`${base}/admin/audit`, { waitUntil: "networkidle" });
    await p2.waitForTimeout(800);
    await snap(p2, `${theme}-10-audit`);
    await p2.goto(`${base}/admin/users`, { waitUntil: "networkidle" });
    await p2.waitForTimeout(900);
    await snap(p2, `${theme}-11-users`);
    await p2.goto(`${base}/`, { waitUntil: "networkidle" });
    await p2.waitForTimeout(700);
    await p2.click("button.app-bar-bell");
    await p2.waitForTimeout(500);
    await snap(p2, `${theme}-12-notif`);
    await p2.close();

    // 移动
    const pm = await (
      await browser.newContext({
        ...devices["iPhone 13"],
        isMobile: true,
        hasTouch: true,
      })
    ).newPage();
    await pm.addInitScript(
      (t) => localStorage.setItem("vfiles:theme", t),
      theme,
    );
    await pm.goto(`${base}/login`, { waitUntil: "networkidle" });
    await pm.fill("#auth-username", SEED_USER.name);
    await pm.fill("#auth-password", SEED_USER.pass);
    await pm.click(".auth-submit");
    await pm.waitForSelector(".file-browser-box", { timeout: 10000 });
    await pm.waitForTimeout(900);
    await snap(pm, `${theme}-13-mobile`);
    await pm.locator('button:has-text("更多")').first().click();
    await pm.waitForTimeout(500);
    await snap(pm, `${theme}-14-mobile-more`);

    // dialog-over-bar 帧（r95/r100 排期债兑现 ✓ 底栏样张终捕）——
    // 两击式（新建 →「新建文件夹」✓ 第二击 = **aria-label 稳式**（title/aria 非文本 ✗✗ hasText 永不中 = r101 崩因））
    // try/catch 隔离（r35 式 ✓ 脆弱帧单败不拖垮全 sweep）
    try {
      // 第一击 = **aria 稳式**（hasText "新建" 坑二连 ✗✗ 底栏钮 aria-label 非文本）
      await pm.locator('.mobile-action-buttons button[aria-label="新建文件夹"]').first().click();
      await pm.waitForTimeout(450);
      await pm.locator('button[aria-label="新建文件夹"]').first().click();
      await pm.waitForTimeout(500);
      await snap(pm, `${theme}-15-dialog-over-bar`);
    } catch (e) {
      console.log(`[ui-sweep] ✗ dialog 帧（${theme}）: ${String(e).slice(0, 80)}`);
    }
    await pm.close();
  }
  await browser.close();

  // MANIFEST 元数据化（r99 ✓ 档案可溯源）
  const meta = [
    `# ui-sweep 档案 ${new Date().toISOString()}`,
    `# 视口: 1440x900（桌面）+ iPhone 13（移动）；主题: light + dark；共 ${shots.length} 张`,
    ...shots,
  ];
  writeFileSync(join(OUT_DIR, "MANIFEST.txt"), meta.join("\n") + "\n");
  log(`完成：${shots.length} 张 → ${OUT_DIR}（MANIFEST.txt 已写）`);
}

main()
  .catch((err) => {
    console.error("[ui-sweep] 失败：", err);
    process.exitCode = 1;
  })
  .finally(teardown);
