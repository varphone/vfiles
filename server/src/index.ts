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
import { UserStore } from "./services/user-store.js";
import { GitServiceManager } from "./services/git-service-manager.js";

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

// 仓库上下文（v1.1.0）
app.use("/api/*", repoContextMiddleware());

app.use("*", errorHandler);

// 健康检查
app.get("/health", (c) => {
  return c.json({ status: "ok", timestamp: new Date().toISOString() });
});

// API路由
app.route("/api/auth", createAuthRoutes(config.auth, userStore));
app.route("/api/files", createFilesRoutes(gitManager));
app.route("/api/history", createHistoryRoutes(gitManager));
app.route("/api/download", createDownloadRoutes(gitManager));
app.route("/api/search", createSearchRoutes(gitManager));

// 静态文件服务（仅生产环境且 dist 存在时启用；开发模式下由 Vite 提供前端）
if (process.env.NODE_ENV === "production") {
  const distIndex = path.resolve(process.cwd(), "client", "dist", "index.html");
  try {
    await fs.access(distIndex);
    app.use("/*", serveStatic({ root: "./client/dist" }));
    app.use("/*", serveStatic({ path: "./client/dist/index.html" }));
  } catch {
    console.warn(
      "未发现 client/dist，将不会提供静态前端资源。请先构建前端：bun run build",
    );
  }
}

console.log(`准备启动 VFiles 服务： http://localhost:${config.port}`);

// 交给 Bun（尤其是 --watch / bun run --watch）来创建/热重载服务器，避免重复 listen
export default {
  port: config.port,
  fetch: app.fetch,
};
