import { Hono } from 'hono';
import { GitService } from '../services/git.service.js';
import { pathSecurityMiddleware } from '../middleware/security.js';

export function createFilesRoutes(gitService: GitService) {
  const app = new Hono();

  /**
   * GET /api/files - 获取文件列表
   */
  app.get('/', pathSecurityMiddleware, async (c) => {
    const path = c.req.query('path') || '';
    const files = await gitService.listFiles(path);

    return c.json({
      success: true,
      data: files,
    });
  });

  /**
   * GET /api/files/content - 获取文件内容
   */
  app.get('/content', pathSecurityMiddleware, async (c) => {
    const path = c.req.query('path');
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

    try {
      const content = await gitService.getFileContent(path, commit);
      
      // 设置响应头
      c.header('Content-Type', 'application/octet-stream');
      c.header('Content-Disposition', `inline; filename="${path.split('/').pop()}"`);
      
      return c.body(content);
    } catch (error) {
      return c.json(
        {
          success: false,
          error: error instanceof Error ? error.message : '读取文件失败',
        },
        404
      );
    }
  });

  /**
   * POST /api/files/upload - 上传文件
   */
  app.post('/upload', async (c) => {
    try {
      const formData = await c.req.formData();
      const file = formData.get('file') as File;
      const path = formData.get('path') as string || '';
      const message = formData.get('message') as string || '上传文件';

      if (!file) {
        return c.json(
          {
            success: false,
            error: '没有文件',
          },
          400
        );
      }

      // 读取文件内容
      const arrayBuffer = await file.arrayBuffer();
      const buffer = Buffer.from(arrayBuffer);

      // 构建完整路径
      const filePath = path ? `${path}/${file.name}` : file.name;

      // 保存文件
      const commitHash = await gitService.saveFile(filePath, buffer, message);

      return c.json({
        success: true,
        data: {
          path: filePath,
          commit: commitHash,
        },
        message: '文件上传成功',
      });
    } catch (error) {
      return c.json(
        {
          success: false,
          error: error instanceof Error ? error.message : '上传失败',
        },
        500
      );
    }
  });

  /**
   * DELETE /api/files - 删除文件
   */
  app.delete('/', pathSecurityMiddleware, async (c) => {
    const path = c.req.query('path');
    const message = c.req.query('message') || '删除文件';

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
      await gitService.deleteFile(path, message);

      return c.json({
        success: true,
        message: '文件删除成功',
      });
    } catch (error) {
      return c.json(
        {
          success: false,
          error: error instanceof Error ? error.message : '删除失败',
        },
        500
      );
    }
  });

  return app;
}
