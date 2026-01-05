import { Hono } from 'hono';
import { GitService } from '../services/git.service.js';
import { pathSecurityMiddleware } from '../middleware/security.js';
import { normalizeRequestPath, parseLimit, validateCommitHash } from '../utils/validation.js';

export function createHistoryRoutes(gitService: GitService) {
  const app = new Hono();

  /**
   * GET /api/history/diff - 获取某个版本的文本 diff
   * 参数：path, commit, parent(可选)
   */
  app.get('/diff', pathSecurityMiddleware, async (c) => {
    const rawPath = c.req.query('path');
    const path = rawPath ? normalizeRequestPath(rawPath) : undefined;
    const commit = c.req.query('commit') || '';
    const parent = c.req.query('parent');

    if (!path) {
      return c.json({ success: false, error: '缺少path参数' }, 400);
    }
    if (!commit || !validateCommitHash(commit)) {
      return c.json({ success: false, error: 'commit 参数无效' }, 400);
    }
    if (parent && !validateCommitHash(parent)) {
      return c.json({ success: false, error: 'parent 参数无效' }, 400);
    }

    try {
      const diffText = await gitService.getFileDiff(path, commit, parent || undefined);
      c.header('Content-Type', 'text/plain; charset=utf-8');
      return c.body(diffText);
    } catch (error) {
      return c.json(
        {
          success: false,
          error: error instanceof Error ? error.message : '获取 diff 失败',
        },
        500
      );
    }
  });

  /**
   * GET /api/history - 获取文件历史
   */
  app.get('/', pathSecurityMiddleware, async (c) => {
    const rawPath = c.req.query('path');
    // 允许 path 为空字符串表示仓库根（用于目录版本浏览）
    const path = rawPath != null ? normalizeRequestPath(rawPath) : undefined;

    const limitResult = parseLimit(c.req.query('limit'), { defaultValue: 50, min: 1, max: 200 });
    if (!limitResult.ok) {
      return c.json({ success: false, error: limitResult.message }, limitResult.status);
    }

    if (path == null) {
      return c.json(
        {
          success: false,
          error: '缺少path参数',
        },
        400
      );
    }

    try {
      const history = await gitService.getFileHistory(path, limitResult.value);

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

    if (!validateCommitHash(hash)) {
      return c.json(
        {
          success: false,
          error: 'hash 参数无效',
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
