import { Context, Next } from "hono";
import { validatePath } from "../utils/path-validator.js";
import { config } from "../config.js";
import {
  isAllowedPathByPrefixes,
  normalizeRequestPath,
} from "../utils/validation.js";

/**
 * 路径安全验证中间件
 */
export function pathSecurityMiddleware(c: Context, next: Next) {
  const raw = c.req.query("path") || c.req.param("path") || "";
  const requestedPath = normalizeRequestPath(raw);

  if (!validatePath(requestedPath, config.repoPath)) {
    return c.json(
      {
        success: false,
        error: "无效的文件路径",
      },
      400,
    );
  }

  if (!isAllowedPathByPrefixes(requestedPath, config.allowedPathPrefixes)) {
    return c.json(
      {
        success: false,
        error: "不允许访问该路径",
      },
      403,
    );
  }

  return next();
}
