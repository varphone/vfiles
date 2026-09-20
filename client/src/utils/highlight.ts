/**
 * 共享的代码高亮加载器。
 *
 * 直接 `import("highlight.js")` 会把全部约 190 种语言打进产物（约 900KB），
 * 而预览通常只涉及常见语言。这里改用官方维护的 `highlight.js/lib/common`
 * 子集（约 35 种语言），在保持预览能力的同时显著减小按需加载体积。
 */
export type HighlightResult = { value: string };

export interface HighlightApi {
  highlightAuto: (code: string, languageSubset?: string[]) => HighlightResult;
  highlight: (
    code: string,
    options: Record<string, unknown>,
  ) => HighlightResult;
  getLanguage?: (name: string) => unknown;
}

/** 语言标记只允许这些字符，避免把围栏信息拼进 class 属性时被注入。 */
function normalizeLanguage(language: string | null | undefined): string {
  return (language ?? "")
    .trim()
    .split(/\s+/)[0]
    .toLowerCase()
    .replace(/[^a-z0-9#+._-]/g, "");
}

/** 扩展名 → highlight.js 语言名（common 子集内）。 */
const EXTENSION_LANGUAGES: Record<string, string> = {
  bash: "bash",
  c: "c",
  cc: "cpp",
  cjs: "javascript",
  conf: "ini",
  cpp: "cpp",
  cs: "csharp",
  css: "css",
  dart: "dart",
  diff: "diff",
  go: "go",
  gql: "graphql",
  graphql: "graphql",
  h: "c",
  hpp: "cpp",
  htm: "xml",
  html: "xml",
  ini: "ini",
  java: "java",
  js: "javascript",
  json: "json",
  jsx: "javascript",
  kt: "kotlin",
  kts: "kotlin",
  less: "less",
  lua: "lua",
  md: "markdown",
  mjs: "javascript",
  php: "php",
  pl: "perl",
  py: "python",
  r: "r",
  rb: "ruby",
  rs: "rust",
  scss: "scss",
  sh: "bash",
  sql: "sql",
  swift: "swift",
  toml: "ini",
  ts: "typescript",
  tsx: "typescript",
  vb: "vbnet",
  vue: "xml",
  xml: "xml",
  yaml: "yaml",
  yml: "yaml",
  zsh: "bash",
};

/** 根据文件名推断 highlight.js 语言名；未知时返回空串（走自动识别）。 */
export function languageForPath(filePath: string): string {
  const name = (filePath.split("/").pop() ?? "").toLowerCase();
  if (name === "dockerfile") return "dockerfile";
  if (name === "makefile") return "makefile";
  const dot = name.lastIndexOf(".");
  if (dot < 0 || dot === name.length - 1) return "";

  return EXTENSION_LANGUAGES[name.slice(dot + 1)] ?? "";
}

/**
 * 高亮一段代码。
 *
 * 指定语言且该语言在 common 子集内时按语言高亮，否则回退到自动识别；
 * highlight.js 的输出已经做过 HTML 转义，可直接插入 DOM。
 */
export function highlightCode(
  api: HighlightApi,
  code: string,
  language?: string | null,
): HighlightResult {
  const lang = normalizeLanguage(language);

  if (lang && api.getLanguage?.(lang)) {
    return api.highlight(code, { language: lang, ignoreIllegals: true });
  }

  return api.highlightAuto(code);
}

/** 渲染完整的 `<pre><code>` 代码块（用于 Markdown 预览）。 */
export function renderHighlightedCode(
  api: HighlightApi,
  code: string,
  language?: string | null,
): string {
  const lang = normalizeLanguage(language);
  const className = lang ? `hljs language-${lang}` : "hljs";

  return `<pre><code class="${className}">${highlightCode(api, code, lang).value}</code></pre>`;
}

/** 判断 Markdown 里是否包含围栏代码块，用于避免无谓地加载高亮包。 */
export function hasFencedCodeBlock(markdown: string): boolean {
  return /^[ \t]*(```|~~~)/m.test(markdown);
}

let cached: HighlightApi | null = null;

export async function loadHighlight(): Promise<HighlightApi> {
  if (cached) return cached;

  const mod = (await import("highlight.js/lib/common")) as unknown as {
    default?: HighlightApi;
  } & HighlightApi;
  cached = mod.default ?? mod;
  return cached;
}
