import { Hono } from 'hono';
import { GitService } from '../services/git.service.js';
import { pathSecurityMiddleware } from '../middleware/security.js';
import { config } from '../config.js';
import { validatePath } from '../utils/path-validator.js';
import {
  isAllowedPathByPrefixes,
  normalizeRequestPath,
  validateRequiredString,
  validateOptionalCommitHash,
  validateOptionalString,
} from '../utils/validation.js';

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
    const requestedPath = normalizeRequestPath(c.req.query('path') || '');
    if (!isAllowedPathByPrefixes(requestedPath, config.allowedPathPrefixes)) {
      return c.json({ success: false, error: '不允许访问该路径' }, 403);
    }

    const files = await gitService.listFiles(requestedPath);

    return c.json({
      success: true,
      data: files,
    });
  });

  /**
   * GET /api/files/content - 获取文件内容
   */
  app.get('/content', pathSecurityMiddleware, async (c) => {
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
      const rawPath = (formData.get('path') as string) || '';
      const requestedPath = normalizeRequestPath(rawPath);
      const messageResult = validateOptionalString(formData.get('message'), 'message', { maxLength: 200 });
      if (!messageResult.ok) {
        return c.json({ success: false, error: messageResult.message }, messageResult.status);
      }
      const message = messageResult.value || '上传文件';

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
      const filePath = requestedPath ? `${requestedPath}/${file.name}` : file.name;

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

      // 文件访问白名单（按最终文件路径判断）
      if (!isAllowedPathByPrefixes(filePath, config.allowedPathPrefixes)) {
        return c.json(
          {
            success: false,
            error: '不允许写入该路径',
          },
          403
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
    const rawPath = c.req.query('path');
    const path = rawPath ? normalizeRequestPath(rawPath) : undefined;

    const messageResult = validateOptionalString(c.req.query('message'), 'message', { maxLength: 200 });
    if (!messageResult.ok) {
      return c.json({ success: false, error: messageResult.message }, messageResult.status);
    }
    const message = messageResult.value || '删除文件';

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

  /**
   * POST /api/files/move - 移动/重命名文件或目录
   */
  app.post('/move', async (c) => {
    let body: any;
    try {
      body = await c.req.json();
    } catch {
      return c.json({ success: false, error: '请求体必须是 JSON' }, 400);
    }

    const fromResult = validateRequiredString(body?.from, 'from', { minLength: 1, maxLength: 500 });
    if (!fromResult.ok) return c.json({ success: false, error: fromResult.message }, fromResult.status);

    const toResult = validateRequiredString(body?.to, 'to', { minLength: 1, maxLength: 500 });
    if (!toResult.ok) return c.json({ success: false, error: toResult.message }, toResult.status);

    const messageResult = validateOptionalString(body?.message, 'message', { maxLength: 200 });
    if (!messageResult.ok) return c.json({ success: false, error: messageResult.message }, messageResult.status);

    const fromPath = normalizeRequestPath(fromResult.value);
    const toPath = normalizeRequestPath(toResult.value);
    const message = messageResult.value || '移动/重命名';

    if (!fromPath || !toPath) {
      return c.json({ success: false, error: 'from/to 不能为空' }, 400);
    }
    if (fromPath === toPath) {
      return c.json({ success: false, error: 'from 和 to 不能相同' }, 400);
    }

    // 路径遍历防护（from/to 都必须在 repo 内）
    if (!validatePath(fromPath, config.repoPath) || !validatePath(toPath, config.repoPath)) {
      return c.json({ success: false, error: '无效的文件路径' }, 400);
    }

    // 文件访问白名单（from/to 都需满足）
    if (
      !isAllowedPathByPrefixes(fromPath, config.allowedPathPrefixes) ||
      !isAllowedPathByPrefixes(toPath, config.allowedPathPrefixes)
    ) {
      return c.json({ success: false, error: '不允许访问该路径' }, 403);
    }

    try {
      const commit = await gitService.movePath(fromPath, toPath, message);
      return c.json({
        success: true,
        data: { from: fromPath, to: toPath, commit },
        message: '移动成功',
      });
    } catch (error) {
      return c.json(
        { success: false, error: error instanceof Error ? error.message : '移动失败' },
        500
      );
    }
  });

  return app;
}
