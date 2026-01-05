import type { Context, Next } from 'hono';

export interface RateLimitOptions {
  enabled: boolean;
  windowMs: number;
  max: number;
}

interface Counter {
  count: number;
  resetAt: number;
}

function getClientIp(c: Context): string {
  const forwardedFor = c.req.header('x-forwarded-for');
  if (forwardedFor) {
    const first = forwardedFor.split(',')[0]?.trim();
    if (first) return first;
  }
  return (
    c.req.header('cf-connecting-ip') ||
    c.req.header('x-real-ip') ||
    c.req.header('x-client-ip') ||
    'unknown'
  );
}

export function rateLimit(options: RateLimitOptions) {
  const enabled = options.enabled;
  const windowMs = Number.isFinite(options.windowMs) && options.windowMs > 0 ? options.windowMs : 60_000;
  const max = Number.isFinite(options.max) && options.max > 0 ? options.max : 120;

  const counters = new Map<string, Counter>();

  return async (c: Context, next: Next) => {
    if (!enabled) return next();

    const ip = getClientIp(c);
    const now = Date.now();

    const current = counters.get(ip);
    if (!current || now >= current.resetAt) {
      counters.set(ip, { count: 1, resetAt: now + windowMs });
    } else {
      current.count += 1;
    }

    const counter = counters.get(ip)!;
    const remaining = Math.max(0, max - counter.count);
    const retryAfterMs = Math.max(0, counter.resetAt - now);

    c.header('X-RateLimit-Limit', String(max));
    c.header('X-RateLimit-Remaining', String(remaining));
    c.header('X-RateLimit-Reset', String(Math.ceil(counter.resetAt / 1000)));

    if (counter.count > max) {
      c.header('Retry-After', String(Math.ceil(retryAfterMs / 1000)));
      return c.json(
        {
          success: false,
          error: '请求过于频繁，请稍后再试',
        },
        429
      );
    }

    // 简单清理：避免 map 无限增长
    if (counters.size > 10_000) {
      for (const [key, value] of counters) {
        if (now >= value.resetAt) counters.delete(key);
      }
    }

    return next();
  };
}
