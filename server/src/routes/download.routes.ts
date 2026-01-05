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

  function pathModuleJoinRepo(requestedPath: string): string {
    return path.join(config.repoPath, requestedPath);
  }

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
      const filename = path.split('/').pop() || 'download';
      c.header('Content-Type', 'application/octet-stream');
      c.header('Content-Disposition', `attachment; filename="${filename}"`);
      c.header('Accept-Ranges', 'bytes');

      // 仅对“当前版本文件”（不带 commit）支持 Range，避免对 git show 输出做随机访问。
      if (!commitResult.value) {
        const fullPath = pathModuleJoinRepo(path);
        const stat = await fs.stat(fullPath);
        if (!stat.isFile()) {
          return c.json({ success: false, error: '目标不是文件' }, 400);
        }

        const total = stat.size;
        const range = c.req.header('range') || c.req.header('Range');
        if (range && /^bytes=\d*-\d*$/i.test(range.trim())) {
          const [, spec] = range.trim().split('=');
          const [startStr, endStr] = spec.split('-');
          let start = startStr ? Number.parseInt(startStr, 10) : 0;
          let end = endStr ? Number.parseInt(endStr, 10) : total - 1;

          if (!Number.isFinite(start) || !Number.isFinite(end) || start < 0 || end < 0 || start > end) {
            c.header('Content-Range', `bytes */${total}`);
            return c.body(null, 416);
          }

          // bytes=-N：从末尾取 N 字节
          if (!startStr && endStr) {
            const suffixLen = Math.min(end, total);
            start = Math.max(0, total - suffixLen);
            end = total - 1;
          }

          if (start >= total) {
            c.header('Content-Range', `bytes */${total}`);
            return c.body(null, 416);
          }
          if (end >= total) end = total - 1;

          const chunkSize = end - start + 1;
          c.header('Content-Range', `bytes ${start}-${end}/${total}`);
          c.header('Content-Length', chunkSize.toString());

          const stream = new ReadableStream<Uint8Array>({
            start(controller) {
              (async () => {
                let file: fs.FileHandle | null = null;
                try {
                  file = await fs.open(fullPath, 'r');
                  const buf = new Uint8Array(chunkSize);
                  const { bytesRead } = await file.read(buf, 0, chunkSize, start);
                  controller.enqueue(buf.subarray(0, bytesRead));
                  controller.close();
                } catch (e) {
                  controller.error(e);
                } finally {
                  try {
                    await file?.close();
                  } catch {
                    // ignore
                  }
                }
              })();
            },
          });

          return c.body(stream, 206);
        }

        // 无 Range：直接返回整文件（仍可续传）
        c.header('Content-Length', total.toString());
        const stream = new ReadableStream<Uint8Array>({
          start(controller) {
            (async () => {
              let file: fs.FileHandle | null = null;
              try {
                file = await fs.open(fullPath, 'r');
                const buf = new Uint8Array(1024 * 1024);
                let offset = 0;
                while (offset < total) {
                  const toRead = Math.min(buf.length, total - offset);
                  const { bytesRead } = await file.read(buf, 0, toRead, offset);
                  if (bytesRead <= 0) break;
                  controller.enqueue(buf.subarray(0, bytesRead));
                  offset += bytesRead;
                }
                controller.close();
              } catch (e) {
                controller.error(e);
              } finally {
                try {
                  await file?.close();
                } catch {
                  // ignore
                }
              }
            })();
          },
        });
        return c.body(stream);
      }

      // 历史版本（commit）：沿用原逻辑
      const commit = commitResult.value;
      const exists = await gitService.fileExistsAtCommit(path, commit);
      if (!exists) {
        return c.json({ success: false, error: '下载失败' }, 404);
      }

      // git show stdout -> ReadableStream，真正流式
      const stream = gitService.getFileContentStreamAtCommit(path, commit);
      return c.body(stream);
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

            // 逐文件、逐块读取并写入 zip，避免将整个文件读入内存
            for (const rel of relFiles) {
              const full = path.join(fullDir, rel);
              const zipPath = `${prefix}/${rel}`.replaceAll('\\', '/');
              const entry = new ZipDeflate(zipPath);
              zip.add(entry);

              let handle: fs.FileHandle | null = null;
              try {
                const st = await fs.stat(full);
                if (!st.isFile()) {
                  // 跳过非文件
                  entry.push(new Uint8Array(0), true);
                  continue;
                }

                handle = await fs.open(full, 'r');
                const buf = new Uint8Array(256 * 1024);
                let offset = 0;
                while (offset < st.size) {
                  const toRead = Math.min(buf.length, st.size - offset);
                  const { bytesRead } = await handle.read(buf, 0, toRead, offset);
                  if (bytesRead <= 0) break;
                  // 必须复制（避免复用同一 buffer 导致数据被覆盖）
                  entry.push(buf.slice(0, bytesRead), false);
                  offset += bytesRead;
                }
                entry.push(new Uint8Array(0), true);
              } finally {
                try {
                  await handle?.close();
                } catch {
                  // ignore
                }
              }
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
