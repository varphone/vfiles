import { Hono } from 'hono';
import { GitService } from '../services/git.service.js';

export function createSearchRoutes(gitService: GitService) {
  const app = new Hono();

  /**
   * GET /api/search - 搜索文件
   */
  app.get('/', async (c) => {
    const query = c.req.query('q');

    if (!query) {
      return c.json(
        {
          success: false,
          error: '缺少搜索关键词',
        },
        400
      );
    }

    try {
      const results = await gitService.searchFiles(query);

      return c.json({
        success: true,
        data: results,
      });
    } catch (error) {
      return c.json(
        {
          success: false,
          error: error instanceof Error ? error.message : '搜索失败',
        },
        500
      );
    }
  });

  return app;
}
