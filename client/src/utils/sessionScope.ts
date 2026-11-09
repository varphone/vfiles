/**
 * 受限会话的路径范围工具。
 *
 * 后端的 roots 是不带首尾 `/` 的命名空间相对路径（如 `docs`），这里统一
 * 归一化后再做前缀判断，避免 `docs2` 被误判为 `docs` 的子目录。
 */

export function normalizeScopePath(path: string | null | undefined): string {
  return String(path ?? "")
    .trim()
    .replace(/\\/g, "/")
    .replace(/^\/+|\/+$/g, "")
    .replace(/\/{2,}/g, "/");
}

export function normalizeScopeRoots(
  roots: readonly string[] | null | undefined,
): string[] {
  const result: string[] = [];
  for (const root of roots ?? []) {
    const value = normalizeScopePath(root);
    if (!value || result.includes(value)) continue;
    result.push(value);
  }
  return result;
}

/** 路径是否位于任一范围根之内（范围根自身也算命中）。 */
export function isPathWithinRoots(
  path: string | null | undefined,
  roots: readonly string[] | null | undefined,
): boolean {
  const normalizedRoots = normalizeScopeRoots(roots);
  if (normalizedRoots.length === 0) return true;
  const value = normalizeScopePath(path);
  return normalizedRoots.some(
    (root) => value === root || value.startsWith(`${root}/`),
  );
}

export function firstScopeRoot(
  roots: readonly string[] | null | undefined,
): string {
  return normalizeScopeRoots(roots)[0] ?? "";
}

/**
 * 受限会话的「上一级」：父目录仍在范围内时返回父路径，否则返回 null
 * （范围根之上没有可回退的入口）。
 */
export function scopedParentPath(
  path: string | null | undefined,
  roots: readonly string[] | null | undefined,
): string | null {
  const value = normalizeScopePath(path);
  if (!value) return null;
  const parent = value.includes("/")
    ? value.slice(0, value.lastIndexOf("/"))
    : "";
  const normalizedRoots = normalizeScopeRoots(roots);
  if (normalizedRoots.length === 0) return parent;
  return isPathWithinRoots(parent, normalizedRoots) ? parent : null;
}

/**
 * 面包屑只保留范围根及其下级，隐藏「根目录」等越权上级入口。
 */
export function sliceBreadcrumbsFromScopeRoot<T extends { path: string }>(
  breadcrumbs: readonly T[],
  roots: readonly string[] | null | undefined,
): T[] {
  const normalizedRoots = normalizeScopeRoots(roots);
  if (normalizedRoots.length === 0) return [...breadcrumbs];
  const index = breadcrumbs.findIndex((crumb) =>
    normalizedRoots.includes(normalizeScopePath(crumb.path)),
  );
  return index >= 0 ? breadcrumbs.slice(index) : [...breadcrumbs];
}

/** 范围根在界面上的短名称（取路径最后一段）。 */
export function scopeRootLabel(root: string): string {
  const value = normalizeScopePath(root);
  return value.split("/").filter(Boolean).pop() || value;
}
