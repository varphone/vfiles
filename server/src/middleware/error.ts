import { Context, Next } from 'hono';

/**
 * 错误处理中间件
 */
export async function errorHandler(c: Context, next: Next) {
  try {
    await next();
  } catch (error) {
    console.error('错误:', error);

    const message = error instanceof Error ? error.message : '服务器内部错误';

    return c.json(
      {
        success: false,
        error: message,
      },
      500
    );
  }
}
