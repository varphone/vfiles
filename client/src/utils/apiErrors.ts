import { formatSize } from "./filePresentation";
/**
 * API 错误文案本地化。
 *
 * 服务端返回稳定的 `code`（如 `PATH_CONFLICT`、`VALIDATION_FAILED`）与英文 `message`，
 * 界面此前直接展示英文，与中文界面不一致。这里按错误码给出中文文案，并在服务端提供了
 * `details`（例如冲突路径）时把它附在文案后面；遇到未知错误码时退回服务端文案，
 * 保证自定义提示不会丢失。
 */

export interface ApiErrorDetails {
  path?: string;
  field?: string;
  reason?: string;
  limit_bytes?: number;
  size_bytes?: number;
}

export interface ApiErrorPayload {
  code?: string;
  message?: string;
  details?: ApiErrorDetails | null;
}

/** 错误码 → 中文文案。 */
export const API_ERROR_MESSAGES: Record<string, string> = {
  NOT_FOUND: "请求的内容不存在",
  ENTRY_NOT_FOUND: "文件或目录不存在",
  VERSION_NOT_FOUND: "该版本不存在",
  SNAPSHOT_NOT_FOUND: "该快照不存在",
  UNAUTHORIZED: "请先登录",
  AUTHENTICATION_FAILED: "认证失败，请重新登录",
  INVALID_CREDENTIALS: "用户名或密码不正确",
  USER_DISABLED: "账号已被禁用，请联系管理员",
  SESSION_EXPIRED: "登录已过期，请重新登录",
  SESSION_REVOKED: "登录状态已失效，请重新登录",
  FORBIDDEN: "没有权限执行该操作",
  PATH_CONFLICT: "目标路径已被占用",
  UPLOAD_CONFLICT: "上传冲突，请刷新后重试",
  UPLOAD_EXPIRED: "上传会话已过期，请重新上传",
  UPLOAD_PART_INVALID: "上传分片无效，请重试",
  STORAGE_QUOTA_EXCEEDED: "存储空间不足",
  SEARCH_INDEX_NOT_READY: "搜索索引尚未就绪，请稍后重试",
  RATE_LIMITED: "操作过于频繁，请稍后重试",
  VALIDATION_FAILED: "输入内容不合法",
  FILE_TOO_LARGE: "文件过大，已超过上限",
  CONFLICT: "操作冲突，请刷新后重试",
  NOT_IMPLEMENTED: "该功能尚未开放",
  INTERNAL_ERROR: "服务器内部错误，请稍后重试",
};

/**
 * 把错误负载转为面向用户的中文文案。
 *
 * @param payload 服务端返回的 `{ code, message, details }`
 * @param fallback 完全无法判断时使用的兜底文案（通常是调用方自己的中文提示）
 */
/** 校验字段名 → 中文标签（服务端只给英文字段名）。 */
export const FIELD_LABELS: Record<string, string> = {
  username: "用户名",
  email: "邮箱",
  password: "密码",
  role: "角色",
  page: "分页参数",
  page_size: "分页参数",
  path: "路径",
  name: "名称",
  message: "备注",
  body: "请求内容",
};

export function localizeApiError(
  payload: ApiErrorPayload | null | undefined,
  fallback: string,
): string {
  const code = payload?.code?.trim().toUpperCase();
  const localized = code ? API_ERROR_MESSAGES[code] : undefined;

  if (localized) {
    // 路径冲突：附上冲突路径
    const path = payload?.details?.path?.trim();
    if (path) return `${localized}：${path}`;

    // 上传超限：附上人类可读的上限
    const limit = payload?.details?.limit_bytes;
    if (code === "FILE_TOO_LARGE" && typeof limit === "number" && limit > 0) {
      return `${localized}（最大 ${formatSize(limit)}）`;
    }

    // 校验失败：指出具体字段（英文原因不直接展示，避免中英混杂）
    const field = payload?.details?.field?.trim();
    if (field) {
      const label = FIELD_LABELS[field.toLowerCase()] ?? field;
      return `${localized}：${label}`;
    }

    return localized;
  }

  // 未知错误码：优先展示服务端文案（可能是面向用户的自定义消息）
  const message = payload?.message?.trim();
  return message || fallback;
}

/** 从 axios 抛出的错误里取出错误负载（兼容 `data` 包裹与旧格式）。 */
export function extractErrorPayload(data: unknown): ApiErrorPayload | null {
  if (!data || typeof data !== "object") return null;

  const record = data as Record<string, unknown>;
  const inner =
    record.data && typeof record.data === "object"
      ? (record.data as Record<string, unknown>)
      : record;

  // 兼容三种形态：`{code,message}`、`{data:{code,message}}`、`{error:{code,message}}`
  const nested =
    inner.error && typeof inner.error === "object"
      ? (inner.error as Record<string, unknown>)
      : inner;

  const payload: ApiErrorPayload = {};
  if (typeof nested.code === "string") payload.code = nested.code;
  if (typeof nested.message === "string") payload.message = nested.message;
  else if (typeof nested.error === "string") payload.message = nested.error;
  if (nested.details && typeof nested.details === "object") {
    payload.details = nested.details as ApiErrorDetails;
  }

  return payload.code || payload.message ? payload : null;
}
