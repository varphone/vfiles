import path from "node:path";

/**
 * 验证路径是否安全，防止路径遍历攻击
 */
export function validatePath(requestedPath: string, basePath: string): boolean {
  if (requestedPath.includes("\0")) return false;

  const normalizedBase = path.resolve(basePath);
  const normalizedPath = path.resolve(basePath, requestedPath);

  // 使用相对路径判断是否越界，避免 startsWith("/data") 误判 "/data2" 等情况
  const rel = path.relative(normalizedBase, normalizedPath);
  if (rel === "") return true;
  if (rel === "..") return false;
  if (rel.startsWith(`..${path.sep}`)) return false;

  return !path.isAbsolute(rel);
}

/**
 * 获取相对路径
 */
export function getRelativePath(fullPath: string, basePath: string): string {
  return path.relative(basePath, fullPath);
}

/**
 * 规范化路径分隔符为正斜杠（用于Git）
 */
export function normalizePathForGit(filePath: string): string {
  return filePath.split(path.sep).join("/");
}
