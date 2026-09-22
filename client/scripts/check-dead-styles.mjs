// 死样式扫描（防回归）：类选择器定义了但全仓库零引用、无动画引用的 keyframes、
// 或样式块花括号不配平 → 报错退出 1。
//
// 方法论与误报豁免沿用 docs/OPTIMIZATION_ROADMAP.md §4.16（三批人工清理的经验）：
//   1. Vue <transition name="X"> 的运行时类（X-enter-*/leave-*/appear-*）由框架生成；
//   2. 模板/脚本里 `` `...${...}` `` 或 'prefix' + x 拼接的类名只保留前缀，
//      以前缀族豁免（如 is-${variant} 的 is-lines、is-audio 等）；
//   3. ALLOW 中是文档化保留类（跨组件/策略类），修改时请同步注释。
//
// 用法：node scripts/check-dead-styles.mjs   （或 bun run lint:styles）

import { readFileSync, readdirSync, statSync } from "node:fs";
import { join, relative, dirname } from "node:path";
import { fileURLToPath } from "node:url";

const ROOT = join(dirname(fileURLToPath(import.meta.url)), "..");

// 文档化保留类（当前为空；新增请注明原因与日期）
const ALLOW = new Set([]);

const walk = (dir, exts, out = []) => {
  for (const name of readdirSync(dir)) {
    const p = join(dir, name);
    const st = statSync(p);
    if (st.isDirectory()) walk(p, exts, out);
    else if (exts.some((e) => name.endsWith(e))) out.push(p);
  }
  return out;
};

const vueFiles = walk(join(ROOT, "src"), [".vue"]);
const tsFiles = [
  ...walk(join(ROOT, "src"), [".ts"]),
  ...(() => {
    try {
      return walk(join(ROOT, "tests"), [".ts"]);
    } catch {
      return [];
    }
  })(),
];
const scssFiles = walk(join(ROOT, "src"), [".scss"]);

// 高亮.js 运行时生成的类（代码块语法高亮）：前缀族 + 两个老式子域
const HLJS_PREFIX = "hljs-";
const HLJS_EXACT = new Set(["function_", "class_"]);

const splitStyle = (text) => {
  const m = text.match(/<style[^>]*>([\s\S]*)<\/style>/i);
  return m
    ? {
        style: m[1],
        body: text.slice(0, m.index) + text.slice(m.index + m[0].length),
      }
    : { style: "", body: text };
};

// ---- 语料（非样式文本）与样式定义 ----
const corpusParts = [];
const styleParts = [];
for (const f of [...vueFiles]) {
  const { style, body } = splitStyle(readFileSync(f, "utf8"));
  corpusParts.push(body);
  if (style) styleParts.push({ file: relative(ROOT, f), style });
}
for (const f of tsFiles) corpusParts.push(readFileSync(f, "utf8"));
for (const f of scssFiles)
  styleParts.push({ file: relative(ROOT, f), style: readFileSync(f, "utf8") });
const corpus = corpusParts.join("\n");

// ---- 豁免 1：transition 运行时类 ----
const transitionNames = new Set();
for (const m of corpus.matchAll(/<[Tt]ransition\b[^>]*?\bname="([^"]+)"/g))
  transitionNames.add(m[1]);
const transitionSuffixes = [
  "enter-from",
  "enter-active",
  "enter-to",
  "leave-from",
  "leave-active",
  "leave-to",
  "appear-from",
  "appear-active",
  "appear-to",
];

// ---- 豁免 2：动态拼接类名的前缀族 ----
const dynPrefixes = new Set();
for (const m of corpus.matchAll(/[`'"]([a-zA-Z][\w-]*?)\$\{/g))
  dynPrefixes.add(m[1]);
for (const m of corpus.matchAll(/['"]([a-zA-Z][\w-]*)['"]\s*\+/g))
  dynPrefixes.add(m[1]);

const isAllowed = (cls) => {
  if (ALLOW.has(cls)) return true;
  if (cls.startsWith(HLJS_PREFIX) || HLJS_EXACT.has(cls)) return true;
  for (const t of transitionNames) {
    if (transitionSuffixes.some((sfx) => cls === `${t}-${sfx}`)) return true;
  }
  for (const p of dynPrefixes) {
    if (p.length > 1 && cls.startsWith(p)) return true;
  }
  return false;
};

const corpusHit = (cls) =>
  new RegExp(
    `(^|[\\s"'` + "`" + `:{\\[(,])${cls}([\\s"'` + "`" + `:.}\\]),;]|$)`,
  ).test(corpus);

// ---- 收集样式里的类定义 / keyframes / 花括号配平 ----
const defined = new Map(); // cls -> file
const keyframes = new Map(); // name -> file
const problems = [];

for (const { file, style } of styleParts) {
  // 花括号配平（排掉字符串/注释里的简单干扰：先去注释）
  const noComments = style
    .replace(/\/\*[\s\S]*?\*\//g, "")
    .replace(/^\s*\/\/.*$/gm, "");
  const opens = (noComments.match(/\{/g) || []).length;
  const closes = (noComments.match(/\}/g) || []).length;
  if (opens !== closes) {
    problems.push(`[brace] ${file}: { ×${opens} vs } ×${closes}（不配平）`);
  }
  for (const m of noComments.matchAll(/@keyframes\s+([\w-]+)/g))
    keyframes.set(m[1], file);
  for (const m of noComments.matchAll(/\.([a-zA-Z][\w-]*)/g)) {
    const cls = m[1];
    if (!defined.has(cls)) defined.set(cls, file);
  }
}

// ---- 判定 ----
let dead = 0;
for (const [cls, file] of defined) {
  if (corpusHit(cls) || isAllowed(cls)) continue;
  problems.push(`[dead-class] ${file}: .${cls}（全仓库零引用）`);
  dead++;
}
const animCorpus = corpus + styleParts.map((s) => s.style).join("\n");
for (const [name, file] of keyframes) {
  const used = new RegExp(`animation(?:-name)?\\s*:[^;}]*${name}\\b`).test(
    animCorpus,
  );
  if (!used) {
    problems.push(
      `[dead-keyframes] ${file}: @keyframes ${name}（无 animation 引用）`,
    );
    dead++;
  }
}

if (problems.length) {
  console.error(`死样式检查未通过（${problems.length} 项）：`);
  for (const p of problems) console.error("  " + p);
  process.exit(1);
}
console.log(
  `死样式检查通过：${defined.size} 个类、${keyframes.size} 组 keyframes、transition 豁免 ${transitionNames.size} 组、动态前缀豁免 ${dynPrefixes.size} 个`,
);
