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
