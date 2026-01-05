import { Context, Next } from 'hono';

/**
 * 日志中间件
 */
export async function logger(c: Context, next: Next) {
  const start = Date.now();
  const method = c.req.method;
  const path = c.req.path;

  await next();

  const duration = Date.now() - start;
  const status = c.res.status;

  console.log(`${method} ${path} ${status} - ${duration}ms`);
}
