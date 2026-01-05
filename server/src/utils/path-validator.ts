import path from 'node:path';
import { normalize } from 'node:path';

/**
 * 验证路径是否安全，防止路径遍历攻击
 */
export function validatePath(requestedPath: string, basePath: string): boolean {
  const normalizedBase = normalize(basePath);
  const normalizedPath = normalize(path.join(basePath, requestedPath));
  
  // 确保请求的路径在基础路径内
  return normalizedPath.startsWith(normalizedBase);
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
  return filePath.split(path.sep).join('/');
}
