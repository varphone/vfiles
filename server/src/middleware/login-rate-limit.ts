import type { Context, Next } from "hono";

export interface LoginRateLimitOptions {
  enabled: boolean;
  windowMs: number;
  max: number;
}

interface Counter {
  count: number;
  resetAt: number;
}

function getClientIp(c: Context): string {
  const forwardedFor = c.req.header("x-forwarded-for");
  if (forwardedFor) {
    const first = forwardedFor.split(",")[0]?.trim();
    if (first) return first;
  }
  return (
    c.req.header("cf-connecting-ip") ||
    c.req.header("x-real-ip") ||
    c.req.header("x-client-ip") ||
    "unknown"
  );
}

function getLoginKey(c: Context): string {
  const ip = getClientIp(c);
  return `${ip}|login`;
}

export function loginRateLimit(options: LoginRateLimitOptions) {
  const enabled = options.enabled;
  const windowMs =
    Number.isFinite(options.windowMs) && options.windowMs > 0
      ? options.windowMs
      : 5 * 60_000;
  const max =
    Number.isFinite(options.max) && options.max > 0 ? options.max : 10;

  // key: ip|login|<username>
  const counters = new Map<string, Counter>();

  return async (c: Context, next: Next) => {
    if (!enabled) return next();

    // 尽量从 body 里拿 username（失败则退化为按 IP）
    let username = "";
    try {
      const body = await c.req.raw.clone().json();
      if (body && typeof body.username === "string") {
        username = body.username.trim().toLowerCase();
      }
    } catch {
      // ignore
    }

    const ipKey = getLoginKey(c);
    const key = username ? `${ipKey}|${username}` : ipKey;

    const now = Date.now();
    const current = counters.get(key);
    if (!current || now >= current.resetAt) {
      counters.set(key, { count: 1, resetAt: now + windowMs });
    } else {
      current.count += 1;
    }

    const counter = counters.get(key)!;
    const remaining = Math.max(0, max - counter.count);
    const retryAfterMs = Math.max(0, counter.resetAt - now);

    c.header("X-Auth-RateLimit-Limit", String(max));
    c.header("X-Auth-RateLimit-Remaining", String(remaining));
    c.header(
      "X-Auth-RateLimit-Reset",
      String(Math.ceil(counter.resetAt / 1000)),
    );

    if (counter.count > max) {
      c.header("Retry-After", String(Math.ceil(retryAfterMs / 1000)));
      return c.json(
        { success: false, error: "登录尝试过于频繁，请稍后再试" },
        429,
      );
    }

    // 简单清理
    if (counters.size > 50_000) {
      for (const [k, v] of counters) {
        if (now >= v.resetAt) counters.delete(k);
      }
    }

    return next();
  };
}
