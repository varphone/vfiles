import { Context, Next } from 'hono';
import { validatePath } from '../utils/path-validator.js';
import { config } from '../config.js';

/**
 * 路径安全验证中间件
 */
export function pathSecurityMiddleware(c: Context, next: Next) {
  const path = c.req.query('path') || c.req.param('path') || '';

  if (!validatePath(path, config.repoPath)) {
    return c.json(
      {
        success: false,
        error: '无效的文件路径',
      },
      400
    );
  }

  return next();
}
