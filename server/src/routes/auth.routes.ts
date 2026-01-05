import { Hono } from "hono";
import { UserStore, verifyPassword } from "../services/user-store.js";
import type { AuthConfig, AuthUserCtx } from "../middleware/auth.js";
import { loginRateLimit } from "../middleware/login-rate-limit.js";
import {
  clearAuthCookie,
  requireAdmin,
  setAuthCookie,
} from "../middleware/auth.js";
import { signAuthToken } from "../utils/auth-token.js";
import { validateRequiredString } from "../utils/validation.js";

function toPublicUser(u: {
  id: string;
  username: string;
  role: string;
  disabled?: boolean;
  createdAt: string;
}) {
  return {
    id: u.id,
    username: u.username,
    role: u.role,
    disabled: u.disabled,
    createdAt: u.createdAt,
  };
}

export function createAuthRoutes(cfg: AuthConfig, userStore: UserStore) {
  const app = new Hono();

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

  return app;
}
