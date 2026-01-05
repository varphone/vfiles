import { Hono } from 'hono';
import { cors } from 'hono/cors';
import { serveStatic } from 'hono/bun';
import fs from 'node:fs/promises';
import path from 'node:path';
import { config } from './config.js';
import { GitService } from './services/git.service.js';
import { errorHandler } from './middleware/error.js';
import { logger } from './middleware/logger.js';
import { createFilesRoutes } from './routes/files.routes.js';
import { createHistoryRoutes } from './routes/history.routes.js';
import { createDownloadRoutes } from './routes/download.routes.js';
import { createSearchRoutes } from './routes/search.routes.js';

const app = new Hono();

// 初始化Git服务
const gitService = new GitService(config.repoPath);
await gitService.initRepo();

console.log('Git仓库初始化完成，路径:', config.repoPath);

// 全局中间件
app.use('*', cors(config.cors));

if (config.enableLogging) {
  app.use('*', logger);
}

app.use('*', errorHandler);

// 健康检查
app.get('/health', (c) => {
  return c.json({ status: 'ok', timestamp: new Date().toISOString() });
});

// API路由
app.route('/api/files', createFilesRoutes(gitService));
app.route('/api/history', createHistoryRoutes(gitService));
app.route('/api/download', createDownloadRoutes(gitService));
app.route('/api/search', createSearchRoutes(gitService));

// 静态文件服务（仅生产环境且 dist 存在时启用；开发模式下由 Vite 提供前端）
if (process.env.NODE_ENV === 'production') {
  const distIndex = path.resolve(process.cwd(), 'client', 'dist', 'index.html');
  try {
    await fs.access(distIndex);
    app.use('/*', serveStatic({ root: './client/dist' }));
    app.use('/*', serveStatic({ path: './client/dist/index.html' }));
  } catch {
    console.warn('未发现 client/dist，将不会提供静态前端资源。请先构建前端：bun run build');
  }
}

console.log(`准备启动 VFiles 服务： http://localhost:${config.port}`);

// 交给 Bun（尤其是 --watch / bun run --watch）来创建/热重载服务器，避免重复 listen
export default {
  port: config.port,
  fetch: app.fetch,
};
