import { Hono } from "hono";
import { UserStore, verifyPassword } from "../services/user-store.js";
import type { AuthConfig, AuthUserCtx } from "../middleware/auth.js";
import { loginRateLimit } from "../middleware/login-rate-limit.js";
import type { EmailService } from "../services/email.service.js";
import {
  clearAuthCookie,
  requireAdmin,
  setAuthCookie,
} from "../middleware/auth.js";
import { signAuthToken } from "../utils/auth-token.js";
import { validateRequiredString } from "../utils/validation.js";
import crypto from "node:crypto";

function toPublicUser(u: {
  id: string;
  username: string;
  email?: string;
  role: string;
  disabled?: boolean;
  createdAt: string;
}) {
  return {
    id: u.id,
    username: u.username,
    email: u.email,
    role: u.role,
    disabled: u.disabled,
    createdAt: u.createdAt,
  };
}

function normalizeEmail(email: string): string {
  return email.trim().toLowerCase();
}

function isValidEmail(email: string): boolean {
  return /^[^\s@]+@[^\s@]+\.[^\s@]+$/.test(email);
}

function getClientIp(c: any): string {
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

interface Counter {
  count: number;
  resetAt: number;
}

function checkRateLimit(
  counters: Map<string, Counter>,
  key: string,
  windowMs: number,
  max: number,
  now: number,
): { ok: true } | { ok: false; retryAfterSec: number } {
  const current = counters.get(key);
  if (!current || now >= current.resetAt) {
    counters.set(key, { count: 1, resetAt: now + windowMs });
  } else {
    current.count += 1;
  }

  const counter = counters.get(key)!;
  if (counter.count > max) {
    const retryAfterMs = Math.max(0, counter.resetAt - now);
    return { ok: false, retryAfterSec: Math.ceil(retryAfterMs / 1000) };
  }
  return { ok: true };
}

function base64UrlEncode(buf: Uint8Array): string {
  return Buffer.from(buf)
    .toString("base64")
    .replaceAll("+", "-")
    .replaceAll("/", "_")
    .replaceAll("=", "");
}

function sha256Hex(s: string): string {
  return crypto.createHash("sha256").update(s).digest("hex");
}

function hmacHex(secret: string, s: string): string {
  return crypto.createHmac("sha256", secret).update(s).digest("hex");
}

export function createAuthRoutes(
  cfg: AuthConfig,
  userStore: UserStore,
  emailService: EmailService,
  opts: { publicBaseUrl?: string } = {},
) {
  const app = new Hono();

  const publicBaseUrl = (opts.publicBaseUrl || "").replace(/\/$/, "");

  // v1.1.2: in-memory one-time stores
  const passwordResetTokens = new Map<
    string,
    { userId: string; expMs: number; used: boolean }
  >();
  const emailLoginCodes = new Map<
    string,
    { codeHash: string; expMs: number; used: boolean; attempts: number }
  >();

  const emailActionCounters = new Map<string, Counter>();

  function gc(now: number) {
    for (const [k, v] of passwordResetTokens) {
      if (v.used || now >= v.expMs) passwordResetTokens.delete(k);
    }
    for (const [k, v] of emailLoginCodes) {
      if (v.used || now >= v.expMs) emailLoginCodes.delete(k);
    }
    if (emailActionCounters.size > 50_000) {
      for (const [k, v] of emailActionCounters) {
        if (now >= v.resetAt) emailActionCounters.delete(k);
      }
    }
  }

  // 登录防爆破（与全局 /api 限流叠加）
  app.use("/login", loginRateLimit(cfg.loginRateLimit));

  app.get("/me", async (c) => {
    if (!cfg.enabled) {
      return c.json({ success: true, data: { enabled: false } });
    }
    const user = (c as any).get("authUser") as AuthUserCtx | undefined;
    return c.json({
      success: true,
      data: {
        enabled: true,
        allowRegister: cfg.allowRegister,
        user: user ?? null,
      },
    });
  });

  app.post("/register", async (c) => {
    if (!cfg.enabled) {
      return c.json({ success: false, error: "未启用认证" }, 400);
    }
    if (!cfg.allowRegister) {
      return c.json({ success: false, error: "已关闭注册" }, 403);
    }

    const body = await c.req.json().catch(() => ({}));
    const usernameR = validateRequiredString(body.username, "username", {
      minLength: 3,
      maxLength: 32,
    });
    if (!usernameR.ok)
      return c.json(
        { success: false, error: usernameR.message },
        usernameR.status,
      );

    const passwordR = validateRequiredString(body.password, "password", {
      minLength: 6,
      maxLength: 200,
    });
    if (!passwordR.ok)
      return c.json(
        { success: false, error: passwordR.message },
        passwordR.status,
      );

    try {
      const created = await userStore.createUser({
        username: usernameR.value,
        password: passwordR.value,
        email: typeof body.email === "string" ? body.email : undefined,
      });

      const exp = Math.floor(Date.now() / 1000) + cfg.tokenTtlSeconds;
      const token = signAuthToken(
        {
          v: 2,
          sub: created.id,
          username: created.username,
          role: created.role,
          exp,
          sv: (created.sessionVersion ?? 0) as number,
        },
        cfg.secret,
      );

      setAuthCookie(c, cfg, token);
      return c.json({ success: true, data: { user: toPublicUser(created) } });
    } catch (e) {
      return c.json(
        { success: false, error: (e as Error).message || "注册失败" },
        400,
      );
    }
  });

  app.post("/login", async (c) => {
    if (!cfg.enabled) {
      return c.json({ success: false, error: "未启用认证" }, 400);
    }

    const body = await c.req.json().catch(() => ({}));
    const usernameR = validateRequiredString(body.username, "username", {
      minLength: 3,
      maxLength: 32,
    });
    if (!usernameR.ok)
      return c.json(
        { success: false, error: usernameR.message },
        usernameR.status,
      );

    const passwordR = validateRequiredString(body.password, "password", {
      minLength: 6,
      maxLength: 200,
    });
    if (!passwordR.ok)
      return c.json(
        { success: false, error: passwordR.message },
        passwordR.status,
      );

    const user = await userStore.getUserByUsername(usernameR.value);
    if (!user || user.disabled) {
      return c.json({ success: false, error: "用户名或密码错误" }, 401);
    }

    const ok = await verifyPassword(passwordR.value, user.passwordHash);
    if (!ok) {
      return c.json({ success: false, error: "用户名或密码错误" }, 401);
    }

    const exp = Math.floor(Date.now() / 1000) + cfg.tokenTtlSeconds;
    const token = signAuthToken(
      {
        v: 2,
        sub: user.id,
        username: user.username,
        role: user.role,
        exp,
        sv: (user.sessionVersion ?? 0) as number,
      },
      cfg.secret,
    );

    setAuthCookie(c, cfg, token);
    return c.json({ success: true, data: { user: toPublicUser(user) } });
  });

  app.post("/logout", async (c) => {
    if (!cfg.enabled) {
      return c.json({ success: true });
    }
    clearAuthCookie(c, cfg);
    return c.json({ success: true });
  });

  // --- Admin: user management (权限管理的基础能力) ---
  app.get("/users", async (c) => {
    if (!cfg.enabled)
      return c.json({ success: false, error: "未启用认证" }, 400);
    const guard = requireAdmin(c);
    if (!guard.ok) return guard.res;

    const users = await userStore.listUsers();
    return c.json({ success: true, data: { users } });
  });

  app.post("/users/:id/role", async (c) => {
    if (!cfg.enabled)
      return c.json({ success: false, error: "未启用认证" }, 400);
    const guard = requireAdmin(c);
    if (!guard.ok) return guard.res;

    const body = await c.req.json().catch(() => ({}));
    const roleR = validateRequiredString(body.role, "role", {
      minLength: 1,
      maxLength: 20,
    });
    if (!roleR.ok)
      return c.json({ success: false, error: roleR.message }, roleR.status);

    const role = roleR.value;
    if (role !== "admin" && role !== "user") {
      return c.json({ success: false, error: "role 仅支持 admin/user" }, 400);
    }

    try {
      await userStore.setUserRole(c.req.param("id"), role);
      return c.json({ success: true });
    } catch (e) {
      return c.json(
        { success: false, error: (e as Error).message || "更新失败" },
        400,
      );
    }
  });

  app.post("/users/:id/disabled", async (c) => {
    if (!cfg.enabled)
      return c.json({ success: false, error: "未启用认证" }, 400);
    const guard = requireAdmin(c);
    if (!guard.ok) return guard.res;

    const body = await c.req.json().catch(() => ({}));
    const disabled = Boolean(body.disabled);

    try {
      await userStore.setUserDisabled(c.req.param("id"), disabled);
      return c.json({ success: true });
    } catch (e) {
      return c.json(
        { success: false, error: (e as Error).message || "更新失败" },
        400,
      );
    }
  });

  // Admin: set user email (为邮件找回/验证码登录提供基础数据)
  app.post("/users/:id/email", async (c) => {
    if (!cfg.enabled)
      return c.json({ success: false, error: "未启用认证" }, 400);
    const guard = requireAdmin(c);
    if (!guard.ok) return guard.res;

    const body = await c.req.json().catch(() => ({}));
    const emailR = validateRequiredString(body.email, "email", {
      minLength: 3,
      maxLength: 200,
    });
    if (!emailR.ok)
      return c.json({ success: false, error: emailR.message }, emailR.status);

    const email = normalizeEmail(emailR.value);
    if (!isValidEmail(email)) {
      return c.json({ success: false, error: "邮箱格式无效" }, 400);
    }

    try {
      await userStore.setUserEmail(c.req.param("id"), email);
      return c.json({ success: true });
    } catch (e) {
      return c.json(
        { success: false, error: (e as Error).message || "更新失败" },
        400,
      );
    }
  });

  // 强制撤销该用户的所有历史会话（使旧 cookie 立刻失效）
  app.post("/users/:id/revoke-sessions", async (c) => {
    if (!cfg.enabled)
      return c.json({ success: false, error: "未启用认证" }, 400);
    const guard = requireAdmin(c);
    if (!guard.ok) return guard.res;

    try {
      await userStore.bumpSessionVersion(c.req.param("id"));
      return c.json({ success: true });
    } catch (e) {
      return c.json(
        { success: false, error: (e as Error).message || "更新失败" },
        400,
      );
    }
  });

  // --- v1.1.2: forgot password ---
  app.post("/password-reset/request", async (c) => {
    if (!cfg.enabled)
      return c.json({ success: false, error: "未启用认证" }, 400);
    if (!emailService.isEnabled())
      return c.json({ success: false, error: "未启用邮件功能" }, 400);

    const now = Date.now();
    gc(now);

    const body = await c.req.json().catch(() => ({}));
    const emailR = validateRequiredString(body.email, "email", {
      minLength: 3,
      maxLength: 200,
    });
    if (!emailR.ok)
      return c.json({ success: false, error: emailR.message }, emailR.status);

    const email = normalizeEmail(emailR.value);
    if (!isValidEmail(email)) {
      // 不泄露：仍返回 success
      return c.json({ success: true });
    }

    const ip = getClientIp(c);
    const rl = checkRateLimit(
      emailActionCounters,
      `${ip}|pwdreset|${email}`,
      10 * 60_000,
      5,
      now,
    );
    if (!rl.ok) {
      c.header("Retry-After", String(rl.retryAfterSec));
      return c.json({ success: false, error: "请求过于频繁，请稍后再试" }, 429);
    }

    const user = await userStore.getUserByEmail(email);
    if (!user || user.disabled) {
      // 不泄露是否存在
      return c.json({ success: true });
    }

    const rawToken = base64UrlEncode(crypto.randomBytes(32));
    const tokenHash = sha256Hex(rawToken);
    passwordResetTokens.set(tokenHash, {
      userId: user.id,
      expMs: now + cfg.passwordResetTokenTtlSeconds * 1000,
      used: false,
    });

    const link = publicBaseUrl
      ? `${publicBaseUrl}/reset-password?token=${encodeURIComponent(rawToken)}`
      : undefined;

    const subject = "重置密码";
    const text = link
      ? `你正在重置 VFiles 账号密码。\n\n请点击链接完成重置：\n${link}\n\n若非本人操作，请忽略本邮件。`
      : `你正在重置 VFiles 账号密码。\n\n重置 token：${rawToken}\n\n请在页面中粘贴 token 并设置新密码。\n\n若非本人操作，请忽略本邮件。`;

    await emailService.sendMail({ to: email, subject, text });
    return c.json({ success: true });
  });

  app.post("/password-reset/confirm", async (c) => {
    if (!cfg.enabled)
      return c.json({ success: false, error: "未启用认证" }, 400);

    const now = Date.now();
    gc(now);

    const body = await c.req.json().catch(() => ({}));
    const tokenR = validateRequiredString(body.token, "token", {
      minLength: 10,
      maxLength: 500,
    });
    if (!tokenR.ok)
      return c.json({ success: false, error: tokenR.message }, tokenR.status);

    const passwordR = validateRequiredString(body.newPassword, "newPassword", {
      minLength: 6,
      maxLength: 200,
    });
    if (!passwordR.ok)
      return c.json(
        { success: false, error: passwordR.message },
        passwordR.status,
      );

    const tokenHash = sha256Hex(tokenR.value.trim());
    const rec = passwordResetTokens.get(tokenHash);
    if (!rec || rec.used || now >= rec.expMs) {
      return c.json({ success: false, error: "token 无效或已过期" }, 400);
    }
    rec.used = true;
    passwordResetTokens.set(tokenHash, rec);

    try {
      await userStore.resetPassword(rec.userId, passwordR.value);
      return c.json({ success: true });
    } catch (e) {
      return c.json(
        { success: false, error: (e as Error).message || "重置失败" },
        400,
      );
    } finally {
      passwordResetTokens.delete(tokenHash);
    }
  });

  // --- v1.1.2: email code login ---
  app.post("/email-login/request", async (c) => {
    if (!cfg.enabled)
      return c.json({ success: false, error: "未启用认证" }, 400);
    if (!emailService.isEnabled())
      return c.json({ success: false, error: "未启用邮件功能" }, 400);

    const now = Date.now();
    gc(now);

    const body = await c.req.json().catch(() => ({}));
    const emailR = validateRequiredString(body.email, "email", {
      minLength: 3,
      maxLength: 200,
    });
    if (!emailR.ok)
      return c.json({ success: false, error: emailR.message }, emailR.status);

    const email = normalizeEmail(emailR.value);
    if (!isValidEmail(email)) {
      return c.json({ success: true });
    }

    const ip = getClientIp(c);
    const rl = checkRateLimit(
      emailActionCounters,
      `${ip}|emaillogin|${email}`,
      10 * 60_000,
      8,
      now,
    );
    if (!rl.ok) {
      c.header("Retry-After", String(rl.retryAfterSec));
      return c.json({ success: false, error: "请求过于频繁，请稍后再试" }, 429);
    }

    const user = await userStore.getUserByEmail(email);
    if (!user || user.disabled) {
      // 不泄露
      return c.json({ success: true });
    }

    const code = String(Math.floor(100000 + Math.random() * 900000));
    const codeHash = hmacHex(cfg.secret, `${email}|${code}`);
    emailLoginCodes.set(email, {
      codeHash,
      expMs: now + cfg.emailLoginCodeTtlSeconds * 1000,
      used: false,
      attempts: 0,
    });

    const subject = "登录验证码";
    const text = `你的 VFiles 登录验证码为：${code}\n\n有效期 ${Math.floor(cfg.emailLoginCodeTtlSeconds / 60)} 分钟。\n若非本人操作，请忽略本邮件。`;
    await emailService.sendMail({ to: email, subject, text });

    return c.json({ success: true });
  });

  app.post("/email-login/verify", async (c) => {
    if (!cfg.enabled)
      return c.json({ success: false, error: "未启用认证" }, 400);

    const now = Date.now();
    gc(now);

    const body = await c.req.json().catch(() => ({}));
    const emailR = validateRequiredString(body.email, "email", {
      minLength: 3,
      maxLength: 200,
    });
    if (!emailR.ok)
      return c.json({ success: false, error: emailR.message }, emailR.status);

    const codeR = validateRequiredString(body.code, "code", {
      minLength: 4,
      maxLength: 20,
    });
    if (!codeR.ok)
      return c.json({ success: false, error: codeR.message }, codeR.status);

    const email = normalizeEmail(emailR.value);
    const rec = emailLoginCodes.get(email);
    if (!rec || rec.used || now >= rec.expMs) {
      return c.json({ success: false, error: "验证码错误或已过期" }, 401);
    }

    rec.attempts += 1;
    if (rec.attempts > 10) {
      emailLoginCodes.delete(email);
      return c.json({ success: false, error: "验证码错误或已过期" }, 401);
    }

    const gotHash = hmacHex(cfg.secret, `${email}|${codeR.value.trim()}`);
    if (
      !crypto.timingSafeEqual(Buffer.from(gotHash), Buffer.from(rec.codeHash))
    ) {
      emailLoginCodes.set(email, rec);
      return c.json({ success: false, error: "验证码错误或已过期" }, 401);
    }

    const user = await userStore.getUserByEmail(email);
    if (!user || user.disabled) {
      emailLoginCodes.delete(email);
      return c.json({ success: false, error: "验证码错误或已过期" }, 401);
    }

    rec.used = true;
    emailLoginCodes.delete(email);

    const exp = Math.floor(Date.now() / 1000) + cfg.tokenTtlSeconds;
    const token = signAuthToken(
      {
        v: 2,
        sub: user.id,
        username: user.username,
        role: user.role,
        exp,
        sv: (user.sessionVersion ?? 0) as number,
      },
      cfg.secret,
    );

    setAuthCookie(c, cfg, token);
    return c.json({ success: true, data: { user: toPublicUser(user) } });
  });

  return app;
}
