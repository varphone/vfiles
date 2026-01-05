import { Hono } from 'hono';
import { GitService } from '../services/git.service.js';
import { pathSecurityMiddleware } from '../middleware/security.js';
import { normalizeRequestPath, validateOptionalCommitHash } from '../utils/validation.js';

export function createDownloadRoutes(gitService: GitService) {
  const app = new Hono();

  /**
   * GET /api/download - 下载文件
   */
  app.get('/', pathSecurityMiddleware, async (c) => {
    const rawPath = c.req.query('path');
    const path = rawPath ? normalizeRequestPath(rawPath) : undefined;
    const commit = c.req.query('commit');

    if (!path) {
      return c.json(
        {
          success: false,
          error: '缺少path参数',
        },
        400
      );
    }

    const commitResult = validateOptionalCommitHash(commit);
    if (!commitResult.ok) {
      return c.json({ success: false, error: commitResult.message }, commitResult.status);
    }

    try {
      const content = await gitService.getFileContent(path, commitResult.value);
      const filename = path.split('/').pop() || 'download';

      // 设置下载响应头
      c.header('Content-Type', 'application/octet-stream');
      c.header('Content-Disposition', `attachment; filename="${filename}"`);
      c.header('Content-Length', content.length.toString());

      const body = content.buffer.slice(content.byteOffset, content.byteOffset + content.byteLength) as ArrayBuffer;
      return c.body(body);
    } catch (error) {
      return c.json(
        {
          success: false,
          error: error instanceof Error ? error.message : '下载失败',
        },
        404
      );
    }
  });

  return app;
}
