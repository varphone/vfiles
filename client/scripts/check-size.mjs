// bundle 预算守护（r93）：dist 独立复算 raw/br/gz（node:zlib ✓ 不依赖 precompress 输出解析）
// 对 scripts/size-budgets.json 预算表 ±tolerancePct 比对；超限 exit 1。
// 用法：bun run size:check（构建后跑）；SIZE_BUDGET_SCALE=0.5 可收紧预算（自测超限路径）。
import { readdirSync, readFileSync, statSync } from "node:fs";
import { join, dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { gzipSync, brotliCompressSync } from "node:zlib";

const ROOT = join(dirname(fileURLToPath(import.meta.url)), "..");
const DIST = resolve(ROOT, "dist");
const budget = JSON.parse(
  readFileSync(join(ROOT, "scripts", "size-budgets.json"), "utf8"),
);
const scale = Number(process.env.SIZE_BUDGET_SCALE || "1");

function walk(dir) {
  const out = [];
  for (const name of readdirSync(dir)) {
    const p = join(dir, name);
    const st = statSync(p);
    if (st.isDirectory()) out.push(...walk(p));
    else out.push(p);
  }
  return out;
}

let raw = 0;
let br = 0;
let gz = 0;
for (const file of walk(DIST)) {
  const buf = readFileSync(file);
  raw += buf.length;
  br += brotliCompressSync(buf).length;
  gz += gzipSync(buf).length;
}
const kb = (n) => (n / 1024).toFixed(1);
const rows = [
  ["rawKB", raw / 1024],
  ["brKB", br / 1024],
  ["gzKB", gz / 1024],
];
const tol = budget.tolerancePct / 100;
let failed = false;
for (const [key, value] of rows) {
  const limit = budget[key] * scale * (1 + tol);
  const over = value > limit;
  if (over) failed = true;
  console.log(
    `[size:check] ${key}: ${kb(value * 1024)} KB / 预算 ${kb(limit * 1024)} KB ${over ? "✗ 超限" : "✓"}`,
  );
}
if (failed) {
  console.error("[size:check] 超出预算（tolerance " + budget.tolerancePct + "%）——精简或上调预算并说明");
  process.exit(1);
}
console.log("[size:check] 全部在预算内 ✓");
