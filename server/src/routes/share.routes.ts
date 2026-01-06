import { Hono } from "hono";
import { config } from "../config.js";
import { getRepoContext } from "../middleware/repo-context.js";
import { normalizeRequestPath } from "../utils/validation.js";
import crypto from "node:crypto";

/**
 * 分享信息结构
 */
export interface ShareInfo {
  /** 短码 */
  code: string;
  /** 仓库路径（用于多用户模式） */
  repoPath: string;
  /** 文件路径 */
  filePath: string;
  /** 可选：固定的 commit hash */
  commit?: string;
  /** 分享者用户名（可选，用于审计） */
  sharedBy?: string;
  /** 过期时间（毫秒时间戳） */
  expireAt: number;
  /** 创建时间（毫秒时间戳） */
  createdAt: number;
}

/**
 * 短码存储（内存）
 * key: 短码, value: 分享信息
 */
const shareStore = new Map<string, ShareInfo>();

/**
 * 短码字符集（URL 安全，避免混淆字符）
 */
const SHORT_CODE_CHARS =
  "23456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghjkmnpqrstuvwxyz";
const SHORT_CODE_LENGTH = 8;

/**
 * 生成短码
 */
function generateShortCode(): string {
  const bytes = crypto.randomBytes(SHORT_CODE_LENGTH);
  let code = "";
  for (let i = 0; i < SHORT_CODE_LENGTH; i++) {
    code += SHORT_CODE_CHARS[bytes[i] % SHORT_CODE_CHARS.length];
  }
  return code;
}

/**
 * 生成唯一短码（避免冲突）
 */
function generateUniqueShortCode(): string {
  let code: string;
  let attempts = 0;
  do {
    code = generateShortCode();
    attempts++;
    if (attempts > 100) {
      throw new Error("无法生成唯一短码");
    }
  } while (shareStore.has(code));
  return code;
}

/**
 * 清理过期的分享
 */
function cleanupExpiredShares(): void {
  const now = Date.now();
  for (const [code, info] of shareStore) {
    if (info.expireAt <= now) {
      shareStore.delete(code);
    }
  }
}

// 每 10 分钟清理一次过期分享
setInterval(cleanupExpiredShares, 10 * 60 * 1000);

/**
 * 通过短码获取分享信息（导出给 download.routes 使用）
 */
export function getShareByCode(code: string): ShareInfo | null {
  const info = shareStore.get(code);
  if (!info) return null;

  // 检查是否过期
  if (info.expireAt <= Date.now()) {
    shareStore.delete(code);
    return null;
  }

  return info;
}

/**
 * 兼容旧的 parseShareToken 接口（用于 download.routes）
 * 现在 token 就是短码
 */
export function parseShareToken(token: string): {
  repoPath: string;
  filePath: string;
  commit?: string;
  sharedBy?: string;
  exp: number;
} | null {
  const info = getShareByCode(token);
  if (!info) return null;

  return {
    repoPath: info.repoPath,
    filePath: info.filePath,
    commit: info.commit,
    sharedBy: info.sharedBy,
    exp: Math.floor(info.expireAt / 1000),
  };
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

    // 清理过期分享
    cleanupExpiredShares();

    // 生成短码
    const code = generateUniqueShortCode();
    const now = Date.now();
    const expireAt = now + ttl * 1000;

    // 存储分享信息
    const shareInfo: ShareInfo = {
      code,
      repoPath,
      filePath,
      commit,
      sharedBy: user?.username,
      expireAt,
      createdAt: now,
    };
    shareStore.set(code, shareInfo);

    // 构建分享链接（使用简洁的短链接格式）
    const baseUrl =
      config.email.publicBaseUrl || `http://localhost:${config.port}`;
    const shareUrl = `${baseUrl}/s/${code}`;

    return c.json({
      success: true,
      data: {
        code,
        url: shareUrl,
        expiresIn: ttl,
        expiresAt: new Date(expireAt).toISOString(),
      },
    });
  });

  /**
   * GET /api/share/info - 获取分享链接信息（不需要登录）
   *
   * Query: code - 分享短码
   */
  app.get("/info", async (c) => {
    const code = c.req.query("code") || c.req.query("token");
    if (!code) {
      return c.json({ success: false, error: "缺少 code 参数" }, 400);
    }

    const info = getShareByCode(code);
    if (!info) {
      return c.json({ success: false, error: "无效或已过期的分享链接" }, 400);
    }

    return c.json({
      success: true,
      data: {
        filePath: info.filePath,
        commit: info.commit,
        sharedBy: info.sharedBy,
        expiresAt: new Date(info.expireAt).toISOString(),
      },
    });
  });

  /**
   * GET /api/share/:code - 重定向到下载（短链接入口）
   */
  app.get("/:code", async (c) => {
    const code = c.req.param("code");
    const info = getShareByCode(code);

    if (!info) {
      return c.json({ success: false, error: "无效或已过期的分享链接" }, 404);
    }

    // 重定向到下载接口
    const downloadUrl = `/api/download?path=${encodeURIComponent(info.filePath)}&share_token=${encodeURIComponent(code)}`;
    return c.redirect(downloadUrl);
  });

  return app;
}
