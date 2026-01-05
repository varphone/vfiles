import path from 'node:path';
import type { ContentfulStatusCode } from 'hono/utils/http-status';

export type ValidationResult<T> =
  | { ok: true; value: T }
  | { ok: false; status: ContentfulStatusCode; message: string };

export function normalizeRequestPath(input: string): string {
  // 统一分隔符与前导斜杠；保持相对路径语义
  let p = input.trim().replaceAll('\\', '/');
  while (p.startsWith('/')) p = p.slice(1);
  // 防御性：去掉重复斜杠
  p = p.replace(/\/+/, '/');
  // 规范化 '.' 段（使用 posix，避免 Windows 盘符语义）
  p = path.posix.normalize(p);
  if (p === '.') return '';
  return p;
}

export function isAllowedPathByPrefixes(requestedPath: string, allowedPrefixes: string[]): boolean {
  if (!allowedPrefixes || allowedPrefixes.length === 0) return true;

  const requested = normalizeRequestPath(requestedPath);
  return allowedPrefixes.some((rawPrefix) => {
    const prefix = normalizeRequestPath(rawPrefix);
    if (!prefix) return true;
    return requested === prefix || requested.startsWith(prefix + '/');
  });
}

export function parseLimit(
  input: string | undefined,
  opts?: { defaultValue?: number; min?: number; max?: number }
): ValidationResult<number> {
  const defaultValue = opts?.defaultValue ?? 50;
  const min = opts?.min ?? 1;
  const max = opts?.max ?? 200;

  if (input == null || input === '') {
    return { ok: true, value: defaultValue };
  }

  const value = Number.parseInt(input, 10);
  if (!Number.isFinite(value)) {
    return { ok: false, status: 400, message: 'limit 参数无效' };
  }
  if (value < min || value > max) {
    return { ok: false, status: 400, message: `limit 必须在 ${min} 到 ${max} 之间` };
  }

  return { ok: true, value };
}

export function validateCommitHash(hash: string): boolean {
  // 允许短 hash（>=7）或完整 40 位
  return /^[0-9a-f]{7,40}$/i.test(hash);
}

export function validateOptionalCommitHash(hash: string | undefined): ValidationResult<string | undefined> {
  if (!hash) return { ok: true, value: undefined };
  if (!validateCommitHash(hash)) {
    return { ok: false, status: 400, message: 'commit 参数无效' };
  }
  return { ok: true, value: hash };
}

export function validateRequiredString(
  input: unknown,
  name: string,
  opts?: { minLength?: number; maxLength?: number }
): ValidationResult<string> {
  const minLength = opts?.minLength ?? 1;
  const maxLength = opts?.maxLength ?? 500;

  if (typeof input !== 'string') {
    return { ok: false, status: 400, message: `缺少${name}参数` };
  }

  const value = input.trim();
  if (value.length < minLength) {
    return { ok: false, status: 400, message: `${name} 不能为空` };
  }
  if (value.length > maxLength) {
    return { ok: false, status: 400, message: `${name} 过长` };
  }

  return { ok: true, value };
}

export function validateOptionalString(
  input: unknown,
  name: string,
  opts?: { maxLength?: number }
): ValidationResult<string | undefined> {
  const maxLength = opts?.maxLength ?? 500;

  if (input == null) return { ok: true, value: undefined };
  if (typeof input !== 'string') {
    return { ok: false, status: 400, message: `${name} 参数无效` };
  }

  const value = input.trim();
  if (value.length === 0) return { ok: true, value: undefined };
  if (value.length > maxLength) {
    return { ok: false, status: 400, message: `${name} 过长` };
  }

  return { ok: true, value };
}
