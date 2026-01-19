import { Hono } from "hono";
import { cors } from "hono/cors";
import { serveStatic } from "hono/bun";
import fs from "node:fs/promises";
import path from "node:path";
import { config } from "./config.js";
import { errorHandler } from "./middleware/error.js";
import { logger } from "./middleware/logger.js";
import { rateLimit } from "./middleware/rate-limit.js";
import { authMiddleware } from "./middleware/auth.js";
import { repoContextMiddleware } from "./middleware/repo-context.js";
import { createFilesRoutes } from "./routes/files.routes.js";
import { createHistoryRoutes } from "./routes/history.routes.js";
import { createDownloadRoutes } from "./routes/download.routes.js";
import { createSearchRoutes } from "./routes/search.routes.js";
import { createAuthRoutes } from "./routes/auth.routes.js";
import { createShareRoutes } from "./routes/share.routes.js";
import { UserStore } from "./services/user-store.js";
import { GitServiceManager } from "./services/git-service-manager.js";
import { EmailService } from "./services/email.service.js";

function toAbs(p: string) {
  return path.isAbsolute(p) ? p : path.resolve(process.cwd(), p);
}

async function exists(p: string) {
  try {
    await fs.access(p);
    return true;
  } catch {
    return false;
  }
}

function getRuntimeLabel() {
  const bunVersion =
    typeof (globalThis as unknown as { Bun?: { version?: string } }).Bun !==
    "undefined"
      ? (globalThis as unknown as { Bun?: { version?: string } }).Bun?.version
      : undefined;

  return bunVersion ? `Bun ${bunVersion}` : `Node ${process.version}`;
}

async function detectClientDist() {
  const envPath =
    process.env.VFILES_CLIENT_DIST || process.env.CLIENT_DIST || "";

  const candidates: string[] = [];
  if (envPath.trim()) {
    candidates.push(toAbs(envPath.trim()));
  }

  // 约定：从启动目录（cwd）寻找前端构建产物。
  candidates.push(path.resolve(process.cwd(), "client", "dist"));
  // 兼容：部分发布结构可能是 dist/client/dist
  candidates.push(path.resolve(process.cwd(), "dist", "client", "dist"));

  for (const distRoot of candidates) {
    const distIndex = path.resolve(distRoot, "index.html");
    if (await exists(distIndex)) {
      return { enabled: true as const, distRoot, distIndex, candidates };
    }
  }

  return { enabled: false as const, candidates };
}

function printStartupInfo(params: {
  staticEnabled: boolean;
  staticDistRoot?: string;
  staticCandidates: string[];
}) {
  const listen = `0.0.0.0:${config.port}`;
  const localUrl = `http://localhost:${config.port}`;
  const publicBaseUrl = config.email.publicBaseUrl || "";

  console.log("\n=== VFiles 启动信息 ===");
  console.log(
    `Runtime: ${getRuntimeLabel()} / ${process.platform} ${process.arch} / pid=${process.pid}`,
  );
  console.log(`NODE_ENV=${process.env.NODE_ENV || ""}  PORT=${process.env.PORT || ""}`);
  console.log(`Listen: ${listen}  (本机: ${localUrl})`);
  console.log(`CWD: ${process.cwd()}`);
  console.log(`Repo: mode=${config.repoMode}  path=${config.repoPath}`);
  console.log(
    `MultiUser: ${config.multiUser.enabled ? "on" : "off"}${config.multiUser.enabled ? `  baseDir=${config.multiUser.baseDir}` : ""}`,
  );
  console.log(
    `Auth: ${config.auth.enabled ? "on" : "off"}  cookie=${config.auth.cookieName}  secure=${config.auth.cookieSecure ?? "(auto)"}`,
  );
  console.log(
    `CORS: origin=${config.cors.origin}  credentials=${config.cors.credentials ? "true" : "false"}`,
  );
  console.log(
    `Static: ${params.staticEnabled ? "on" : "off"}${params.staticEnabled ? `  dist=${params.staticDistRoot}` : ""}`,
  );
  if (!params.staticEnabled) {
    console.log("Static 未启用：将不会托管前端页面（建议访问 /health 或 /api/* 进行反代验证）。");
    console.log(
      `静态资源候选路径（任一包含 index.html 即可）：\n- ${params.staticCandidates.join("\n- ")}`,
    );
    console.log(
      "可通过环境变量 VFILES_CLIENT_DIST 或 CLIENT_DIST 指定前端 dist 目录（相对路径基于 CWD）。",
    );
  }

  if (publicBaseUrl) {
    console.log(`PUBLIC_BASE_URL: ${publicBaseUrl}`);
  }

  console.log("Endpoints:");
  console.log("- GET  /health");
  console.log("- GET  /api/*");
  console.log("- GET  /s/:code  (重定向到 /api/share/:code)");
  console.log("\n反向代理排查提示：");
  console.log("- 优先用域名访问 /health，若仍 404 多半是 nginx 未命中 server_name/location");
  console.log("- 确保转发 Host 与 X-Forwarded-Proto/For（否则外链/回调可能异常）");
  console.log("======================\n");
}

const app = new Hono();

if (config.auth.enabled && !config.auth.secret) {
  throw new Error(
    "ENABLE_AUTH=true 时必须设置 AUTH_SECRET（用于签名登录 cookie）",
  );
}

// Git 服务管理器（多用户/单用户统一走 per-request repoContext）
const gitManager = new GitServiceManager();

// 全局中间件
app.use("*", cors(config.cors));

if (config.enableLogging) {
  app.use("*", logger);
}

// 请求频率限制（仅对 API 生效）
app.use("/api/*", rateLimit(config.rateLimit));

// 认证（v1.1.0）
const userStore = new UserStore(config.auth.storagePath);
app.use("/api/*", authMiddleware(config.auth, userStore));

// 邮件服务（v1.1.2）
const emailService = new EmailService({
  enabled:
    config.email.enabled &&
    Boolean(config.email.host) &&
    Boolean(config.email.from),
  host: config.email.host,
  port: config.email.port,
  secure: config.email.secure,
  user: config.email.user || undefined,
  pass: config.email.pass || undefined,
  from: config.email.from,
});

// 仓库上下文（v1.1.0）
app.use("/api/*", repoContextMiddleware());

app.use("*", errorHandler);

// 健康检查
app.get("/health", (c) => {
  return c.json({ status: "ok", timestamp: new Date().toISOString() });
});

// API路由
app.route(
  "/api/auth",
  createAuthRoutes(config.auth, userStore, emailService, {
    publicBaseUrl: config.email.publicBaseUrl,
  }),
);
app.route("/api/files", createFilesRoutes(gitManager));
app.route("/api/history", createHistoryRoutes(gitManager));
app.route("/api/download", createDownloadRoutes(gitManager));
app.route("/api/search", createSearchRoutes(gitManager));
app.route("/api/share", createShareRoutes());

// 短链接重定向 /s/:code -> /api/share/:code
app.get("/s/:code", (c) => {
  const code = c.req.param("code");
  return c.redirect(`/api/share/${code}`);
});

// 静态文件服务（仅生产环境且 dist 存在时启用；开发模式下由 Vite 提供前端）
let staticEnabled = false;
let staticDistRoot: string | undefined;
let staticCandidates: string[] = [];
if (process.env.NODE_ENV === "production") {
  const detected = await detectClientDist();
  staticCandidates = detected.candidates;

  if (detected.enabled) {
    staticEnabled = true;
    staticDistRoot = detected.distRoot;
    app.use("/*", serveStatic({ root: detected.distRoot }));
    app.use("/*", serveStatic({ path: detected.distIndex }));
  } else {
    console.warn(
      "未发现前端构建产物（index.html），将不会提供静态前端资源。",
    );
  }
}

app.notFound((c) => {
  if (config.enableLogging) {
    const host = c.req.header("host") || "";
    const xfp = c.req.header("x-forwarded-proto") || "";
    const xff = c.req.header("x-forwarded-for") || "";
    console.warn(
      `[notFound] ${c.req.method} ${c.req.path} host=${host} xfp=${xfp} xff=${xff}`,
    );
  }
  return c.text("Not Found", 404);
});

printStartupInfo({
  staticEnabled,
  staticDistRoot,
  staticCandidates,
});

// 交给 Bun（尤其是 --watch / bun run --watch）来创建/热重载服务器，避免重复 listen
export default {
  port: config.port,
  fetch: app.fetch,
};
