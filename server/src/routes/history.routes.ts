import { Hono } from 'hono';
import { GitService } from '../services/git.service.js';
import { pathSecurityMiddleware } from '../middleware/security.js';

export function createHistoryRoutes(gitService: GitService) {
  const app = new Hono();

  /**
   * GET /api/history - 获取文件历史
   */
  app.get('/', pathSecurityMiddleware, async (c) => {
    const path = c.req.query('path');
    const limit = parseInt(c.req.query('limit') || '50');

    if (!path) {
      return c.json(
        {
          success: false,
          error: '缺少path参数',
        },
        400
      );
    }

    try {
      const history = await gitService.getFileHistory(path, limit);

      return c.json({
        success: true,
        data: history,
      });
    } catch (error) {
      return c.json(
        {
          success: false,
          error: error instanceof Error ? error.message : '获取历史失败',
        },
        500
      );
    }
  });

  /**
   * GET /api/history/commit/:hash - 获取提交详情
   */
  app.get('/commit/:hash', async (c) => {
    const hash = c.req.param('hash');

    if (!hash) {
      return c.json(
        {
          success: false,
          error: '缺少hash参数',
        },
        400
      );
    }

    try {
      const commit = await gitService.getCommitDetails(hash);

      return c.json({
        success: true,
        data: commit,
      });
    } catch (error) {
      return c.json(
        {
          success: false,
          error: error instanceof Error ? error.message : '获取提交详情失败',
        },
        404
      );
    }
  });

  return app;
}
