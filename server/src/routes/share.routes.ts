import { Hono } from "hono";
import { config } from "../config.js";
import { getRepoContext } from "../middleware/repo-context.js";
import { normalizeRequestPath } from "../utils/validation.js";
import crypto from "node:crypto";

/**
 * 分享 token 的 payload 结构
 */
export interface ShareTokenPayload {
  /** 分享类型标识 */
  type: "share";
  /** 仓库路径（用于多用户模式） */
  repoPath: string;
  /** 文件路径 */
  filePath: string;
  /** 可选：固定的 commit hash */
  commit?: string;
  /** 分享者用户名（可选，用于审计） */
  sharedBy?: string;
  /** 过期时间（秒） */
  exp: number;
}

function base64UrlEncode(buf: Uint8Array): string {
  return Buffer.from(buf)
    .toString("base64")
    .replaceAll("+", "-")
    .replaceAll("/", "_")
    .replaceAll("=", "");
}

function base64UrlDecode(s: string): Uint8Array {
  const pad = s.length % 4 === 0 ? "" : "=".repeat(4 - (s.length % 4));
  const b64 = s.replaceAll("-", "+").replaceAll("_", "/") + pad;
  return new Uint8Array(Buffer.from(b64, "base64"));
}

function timingSafeEqual(a: string, b: string): boolean {
  const aBuf = Buffer.from(a);
  const bBuf = Buffer.from(b);
  if (aBuf.length !== bBuf.length) return false;
  return crypto.timingSafeEqual(aBuf, bBuf);
}

/**
 * 签名分享 token
 */
function signShareToken(payload: ShareTokenPayload, secret: string): string {
  const payloadJson = JSON.stringify(payload);
  const payloadB64 = base64UrlEncode(Buffer.from(payloadJson, "utf-8"));
  const sig = crypto.createHmac("sha256", secret).update(payloadB64).digest();
  const sigB64 = base64UrlEncode(sig);
  return `${payloadB64}.${sigB64}`;
}

/**
 * 验证分享 token
 */
function verifyShareToken(
  token: string,
  secret: string,
): ShareTokenPayload | null {
  const parts = (token || "").split(".");
  if (parts.length !== 2) return null;
  const [payloadB64, sigB64] = parts;
  if (!payloadB64 || !sigB64) return null;

  const expected = base64UrlEncode(
    crypto.createHmac("sha256", secret).update(payloadB64).digest(),
  );
  if (!timingSafeEqual(expected, sigB64)) {
    return null;
  }

  let payload: ShareTokenPayload;
  try {
    payload = JSON.parse(
      Buffer.from(base64UrlDecode(payloadB64)).toString("utf-8"),
    );
  } catch {
    return null;
  }

  if (!payload || payload.type !== "share") return null;
  if (typeof payload.exp !== "number") return null;

  const nowSec = Math.floor(Date.now() / 1000);
  if (payload.exp <= nowSec) return null;

  return payload;
}

/**
 * 解析分享 token（导出给 download.routes 使用）
 */
export function parseShareToken(token: string): ShareTokenPayload | null {
  return verifyShareToken(token, config.auth.secret);
}

export function createShareRoutes() {
  const app = new Hono();

  /**
   * POST /api/share - 创建分享链接
   *
   * Body: {
   *   path: string;      // 要分享的文件路径
   *   commit?: string;   // 可选：固定到某个版本
   *   ttl?: number;      // 可选：有效期（秒），默认使用配置的 defaultTtlSeconds
   * }
   */
  app.post("/", async (c) => {
    // 检查是否启用分享功能
    if (!config.share.enabled) {
      return c.json({ success: false, error: "分享功能未启用" }, 403);
    }

    // 必须登录才能创建分享（多用户模式下需要知道是谁的仓库）
    const user = (c as any).get("authUser") as { username: string } | undefined;

    // 单用户模式下不强制登录，但多用户模式必须登录
    if (config.multiUser.enabled && !user) {
      return c.json({ success: false, error: "请先登录" }, 401);
    }

    const body = await c.req.json().catch(() => ({}));
    const rawPath = body.path as string | undefined;
    const commit = body.commit as string | undefined;
    let ttl = body.ttl as number | undefined;

    if (!rawPath) {
      return c.json({ success: false, error: "缺少 path 参数" }, 400);
    }

    const filePath = normalizeRequestPath(rawPath);

    // 验证 TTL
    if (ttl !== undefined) {
      if (typeof ttl !== "number" || ttl <= 0) {
        return c.json({ success: false, error: "无效的 ttl 参数" }, 400);
      }
      if (ttl > config.share.maxTtlSeconds) {
        ttl = config.share.maxTtlSeconds;
      }
    } else {
      ttl = config.share.defaultTtlSeconds;
    }

    // 获取当前用户的仓库路径
    const { repoPath } = getRepoContext(c);

    // 计算过期时间
    const exp = Math.floor(Date.now() / 1000) + ttl;

    // 创建分享 token
    const payload: ShareTokenPayload = {
      type: "share",
      repoPath,
      filePath,
      commit,
      sharedBy: user?.username,
      exp,
    };

    const token = signShareToken(payload, config.auth.secret);

    // 构建分享链接
    const baseUrl =
      config.email.publicBaseUrl || `http://localhost:${config.port}`;
    const shareUrl = `${baseUrl}/api/download?path=${encodeURIComponent(filePath)}&share_token=${encodeURIComponent(token)}`;

    return c.json({
      success: true,
      data: {
        token,
        url: shareUrl,
        expiresIn: ttl,
        expiresAt: new Date(Date.now() + ttl * 1000).toISOString(),
      },
    });
  });

  /**
   * GET /api/share/info - 获取分享链接信息（不需要登录）
   *
   * Query: token - 分享 token
   */
  app.get("/info", async (c) => {
    const token = c.req.query("token");
    if (!token) {
      return c.json({ success: false, error: "缺少 token 参数" }, 400);
    }

    const payload = parseShareToken(token);
    if (!payload) {
      return c.json({ success: false, error: "无效或已过期的分享链接" }, 400);
    }

    return c.json({
      success: true,
      data: {
        filePath: payload.filePath,
        commit: payload.commit,
        sharedBy: payload.sharedBy,
      },
    });
  });

  return app;
}
