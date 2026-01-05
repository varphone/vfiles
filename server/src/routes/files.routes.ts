import { Hono } from 'hono';
import { GitService } from '../services/git.service.js';
import { pathSecurityMiddleware } from '../middleware/security.js';
import { config } from '../config.js';
import { validatePath } from '../utils/path-validator.js';

export function createFilesRoutes(gitService: GitService) {
  const app = new Hono();

  function getFileExtension(filename: string): string {
    const lastDot = filename.lastIndexOf('.');
    if (lastDot <= 0 || lastDot === filename.length - 1) return '';
    return filename.slice(lastDot + 1).toLowerCase();
  }

  function isAllowedFileType(file: File): boolean {
    const allowed = config.allowedFileTypes;
    if (!allowed || allowed.length === 0) return true;

    const ext = getFileExtension(file.name);
    const mime = (file.type || '').toLowerCase();

    return allowed.some((rule) => {
      const normalized = rule.trim().toLowerCase();
      if (!normalized) return false;
      // mime 规则（如 image/png）
      if (normalized.includes('/')) return normalized === mime;
      // 扩展名规则（允许 "txt" 或 ".txt"）
      const normalizedExt = normalized.startsWith('.') ? normalized.slice(1) : normalized;
      return normalizedExt === ext;
    });
  }

  function isSafeUploadFilename(filename: string): boolean {
    // 只允许“文件名”本身，禁止携带路径分隔符/父目录跳转
    if (!filename) return false;
    if (filename.includes('/') || filename.includes('\\')) return false;
    if (filename === '.' || filename === '..') return false;
    return true;
  }

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
      
      const body = content.buffer.slice(content.byteOffset, content.byteOffset + content.byteLength) as ArrayBuffer;
      return c.body(body);
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

      // 文件名与路径安全校验
      if (!isSafeUploadFilename(file.name)) {
        return c.json(
          {
            success: false,
            error: '非法文件名',
          },
          400
        );
      }

      // 文件大小限制（Bun File.size 为字节）
      if (file.size > config.maxFileSize) {
        return c.json(
          {
            success: false,
            error: `文件过大，最大允许 ${(config.maxFileSize / (1024 * 1024)).toFixed(0)}MB`,
          },
          413
        );
      }

      // 文件类型/扩展名白名单
      if (!isAllowedFileType(file)) {
        return c.json(
          {
            success: false,
            error: '不允许的文件类型',
          },
          415
        );
      }

      // 读取文件内容
      const arrayBuffer = await file.arrayBuffer();
      const buffer = Buffer.from(arrayBuffer);

      // 构建完整路径
      const filePath = path ? `${path}/${file.name}` : file.name;

      // 防止 path 穿越（确保最终写入路径在 repo 内）
      if (!validatePath(filePath, config.repoPath)) {
        return c.json(
          {
            success: false,
            error: '无效的文件路径',
          },
          400
        );
      }

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
