import type { Context, Next } from "hono";
import { getCookie, setCookie, deleteCookie } from "hono/cookie";
import { UserStore } from "../services/user-store.js";
import { verifyAuthToken } from "../utils/auth-token.js";

export interface AuthConfig {
  enabled: boolean;
  secret: string;
  cookieName: string;
  tokenTtlSeconds: number;
  allowRegister: boolean;
}

export interface AuthUserCtx {
  id: string;
  username: string;
  role: string;
}

export function setAuthCookie(c: Context, cfg: AuthConfig, token: string) {
  // maxAge seconds
  setCookie(c, cfg.cookieName, token, {
    httpOnly: true,
    sameSite: "Lax",
    secure: process.env.NODE_ENV === "production",
    path: "/",
    maxAge: cfg.tokenTtlSeconds,
  });
}

export function clearAuthCookie(c: Context, cfg: AuthConfig) {
  deleteCookie(c, cfg.cookieName, { path: "/" });
}

export function authMiddleware(cfg: AuthConfig, userStore: UserStore) {
  return async (c: Context, next: Next) => {
    if (!cfg.enabled) return next();

    const pathname = new URL(c.req.url).pathname;
    // allow auth endpoints + health
    if (pathname.startsWith("/api/auth")) return next();
    if (pathname === "/health") return next();

    const token = getCookie(c, cfg.cookieName);
    if (!token) {
      return c.json({ success: false, error: "未登录" }, 401);
    }

    const verified = verifyAuthToken(token, cfg.secret);
    if (!verified.ok) {
      clearAuthCookie(c, cfg);
      return c.json({ success: false, error: "登录已过期" }, 401);
    }

    const user = await userStore.getUserById(verified.payload.sub);
    if (!user || user.disabled) {
      clearAuthCookie(c, cfg);
      return c.json({ success: false, error: "用户不可用" }, 401);
    }

    c.set("authUser", {
      id: user.id,
      username: user.username,
      role: user.role,
    } satisfies AuthUserCtx);

    return next();
  };
}

export function requireAdmin(c: Context): { ok: true } | { ok: false; res: Response } {
  const user = c.get("authUser") as AuthUserCtx | undefined;
  if (!user) {
    return { ok: false, res: c.json({ success: false, error: "未登录" }, 401) };
  }
  if (user.role !== "admin") {
    return { ok: false, res: c.json({ success: false, error: "需要管理员权限" }, 403) };
  }
  return { ok: true };
}
