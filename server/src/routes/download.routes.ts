import { Hono } from 'hono';
import { GitService } from '../services/git.service.js';
import { pathSecurityMiddleware } from '../middleware/security.js';
import { normalizeRequestPath, validateOptionalCommitHash } from '../utils/validation.js';
import fs from 'node:fs/promises';
import path from 'node:path';
import { config } from '../config.js';
import { Zip, ZipDeflate } from 'fflate';

export function createDownloadRoutes(gitService: GitService) {
  const app = new Hono();

  async function listFilesRecursively(baseDirFullPath: string, relDir: string): Promise<string[]> {
    const dirFull = path.join(baseDirFullPath, relDir);
    const entries = await fs.readdir(dirFull, { withFileTypes: true });

    const files: string[] = [];
    for (const entry of entries) {
      if (entry.name === '.git') continue;
      const rel = relDir ? `${relDir}/${entry.name}` : entry.name;
      if (entry.isDirectory()) {
        files.push(...(await listFilesRecursively(baseDirFullPath, rel)));
      } else if (entry.isFile()) {
        files.push(rel);
      }
    }
    return files;
  }

  function folderZipName(requestedPath: string): string {
    const name = requestedPath.split('/').filter(Boolean).pop() || 'root';
    return `${name}.zip`;
  }

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

  /**
   * GET /api/download/folder - 下载文件夹（ZIP）
   */
  app.get('/folder', pathSecurityMiddleware, async (c) => {
    const rawPath = c.req.query('path');
    const requestedPath = rawPath ? normalizeRequestPath(rawPath) : '';

    const fullDir = path.join(config.repoPath, requestedPath);
    try {
      const st = await fs.stat(fullDir);
      if (!st.isDirectory()) {
        return c.json({ success: false, error: '目标不是文件夹' }, 400);
      }
    } catch (error) {
      return c.json({ success: false, error: error instanceof Error ? error.message : '读取文件夹失败' }, 404);
    }

    const zipFilename = folderZipName(requestedPath);
    c.header('Content-Type', 'application/zip');
    c.header('Content-Disposition', `attachment; filename="${zipFilename}"`);

    // 使用 ReadableStream 流式输出 zip
    const stream = new ReadableStream<Uint8Array>({
      start(controller) {
        (async () => {
          try {
            const prefix = requestedPath.split('/').filter(Boolean).pop() || 'root';
            const relFiles = await listFilesRecursively(fullDir, '');

            const zip = new Zip((err, data, final) => {
              if (err) {
                controller.error(err);
                return;
              }
              if (data) controller.enqueue(data);
              if (final) controller.close();
            });

            for (const rel of relFiles) {
              const buf = await fs.readFile(path.join(fullDir, rel));
              // zip 内路径统一使用 '/'
              const zipPath = `${prefix}/${rel}`.replaceAll('\\', '/');
              const file = new ZipDeflate(zipPath);
              zip.add(file);
              file.push(new Uint8Array(buf), true);
            }

            zip.end();
          } catch (e) {
            controller.error(e);
          }
        })();
      },
    });

    return c.body(stream);
  });

  return app;
}
