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

  let lastCacheCleanupAt = 0;
  const CACHE_CLEANUP_INTERVAL_MS = 10 * 60 * 1000;

  function makeDownloadCacheKey(commit: string, filePath: string): string {
    // 文件名中避免出现路径分隔符
    const safePath = filePath.replaceAll('..', '').replaceAll('/', '_').replaceAll('\\', '_');
    return `${commit}_${safePath}`;
  }

  async function cleanupDownloadCacheIfNeeded(): Promise<void> {
    const now = Date.now();
    if (now - lastCacheCleanupAt < CACHE_CLEANUP_INTERVAL_MS) return;
    lastCacheCleanupAt = now;

    try {
      await fs.mkdir(config.downloadCacheDir, { recursive: true });
      const entries = await fs.readdir(config.downloadCacheDir, { withFileTypes: true });
      for (const entry of entries) {
        if (!entry.isFile()) continue;
        const p = path.join(config.downloadCacheDir, entry.name);
        try {
          const st = await fs.stat(p);
          if (now - st.mtimeMs > config.downloadCacheTtlMs) {
            await fs.rm(p, { force: true });
          }
        } catch {
          // ignore
        }
      }
    } catch {
      // ignore
    }
  }

  async function ensureMaterializedLfsFile(commit: string, filePath: string): Promise<string> {
    await fs.mkdir(config.downloadCacheDir, { recursive: true });
    const cacheKey = makeDownloadCacheKey(commit, filePath);
    const outPath = path.join(config.downloadCacheDir, cacheKey);

    try {
      const st = await fs.stat(outPath);
      const now = Date.now();
      if (now - st.mtimeMs <= config.downloadCacheTtlMs && st.isFile()) {
        return outPath;
      }
    } catch {
      // cache miss
    }

    // 重新 materialize（smudge 输出到文件），写入过程保持流式
    const stream = gitService.getFileContentSmudgedStreamAtCommit(filePath, commit);
    const tmpPath = `${outPath}.tmp`;
    let handle: fs.FileHandle | null = null;
    const reader = stream.getReader();
    try {
      handle = await fs.open(tmpPath, 'w');
      while (true) {
        const { done, value } = await reader.read();
        if (done) break;
        if (value && value.byteLength > 0) {
          await handle.write(value);
        }
      }
    } finally {
      try {
        await reader.cancel();
      } catch {
        // ignore
      }
      try {
        await handle?.close();
      } catch {
        // ignore
      }
    }

    await fs.rename(tmpPath, outPath);
    return outPath;
  }

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

      // worktree 模式：当前版本直接从磁盘读取并支持 Range。
      // bare 模式：当前版本等同于 HEAD（走 git show / cat-file），同样提供 Range（逻辑 Range 或 LFS 缓存 Range）。
      if (!commitResult.value && config.repoMode !== 'bare') {
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

      // 历史版本（commit）或 bare 模式当前版本（HEAD）：沿用 git 读取逻辑
      const commit = commitResult.value || 'HEAD';
      const exists = await gitService.fileExistsAtCommit(path, commit);
      if (!exists) {
        return c.json({ success: false, error: '下载失败' }, 404);
      }

      // Git LFS：如果 blob 是 pointer，需要先 smudge 还原为真实文件。
      // 为了支持 Range/断点续传，这里把 smudge 结果流式落地到临时文件，再走文件 Range 读取。
      await cleanupDownloadCacheIfNeeded();
      const isLfsPointer = await gitService.isLfsPointerAtCommit(path, commit);
      if (isLfsPointer) {
        const fullPath = await ensureMaterializedLfsFile(commit, path);
        const stat = await fs.stat(fullPath);
        if (!stat.isFile()) {
          return c.json({ success: false, error: '下载失败' }, 404);
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

      // 历史版本支持 Range（通过流式丢弃实现逻辑 Range）
      const total = await gitService.getFileSizeAtCommit(path, commit);
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

        const stream = gitService.getFileContentRangeStreamAtCommit(path, commit, start, end);
        return c.body(stream, 206);
      }

      // 无 Range：也返回可续传
      c.header('Content-Length', total.toString());
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
    const commitParam = c.req.query('commit');

    const commitResult = validateOptionalCommitHash(commitParam);
    if (!commitResult.ok) {
      return c.json({ success: false, error: commitResult.message }, commitResult.status);
    }
    const ref = commitResult.value || 'HEAD';

    // 指定 commit 或 bare 模式：文件不在（或不应读）磁盘工作区，需从对象库读取并打包
    if (config.repoMode === 'bare' || !!commitResult.value) {
      const zipFilename = folderZipName(requestedPath);
      c.header('Content-Type', 'application/zip');
      c.header('Content-Disposition', `attachment; filename="${zipFilename}"`);

      const stream = new ReadableStream<Uint8Array>({
        start(controller) {
          (async () => {
            try {
              const prefix = requestedPath.split('/').filter(Boolean).pop() || 'root';
              const base = requestedPath.replace(/^\/+|\/+$/g, '');

              // 递归列出文件（name-only）
              const args = base
                ? ['git', 'ls-tree', '-r', '-z', '--name-only', ref, '--', base]
                : ['git', 'ls-tree', '-r', '-z', '--name-only', ref];
              const proc = Bun.spawn(args, { cwd: config.repoPath, stdout: 'pipe', stderr: 'pipe' });
              const code = await proc.exited;
              if (code !== 0) {
                const err = await new Response(proc.stderr).text();
                throw new Error(err || '读取文件夹失败');
              }
              const listText = new TextDecoder().decode(
                new Uint8Array(await new Response(proc.stdout).arrayBuffer())
              );
              const filePaths = listText.split('\0').filter(Boolean);

              const zip = new Zip((err, data, final) => {
                if (err) {
                  controller.error(err);
                  return;
                }
                if (data) controller.enqueue(data);
                if (final) controller.close();
              });

              for (const fullRel of filePaths) {
                // fullRel 是仓库根相对路径
                const rel = base ? fullRel.slice(base.length).replace(/^\//, '') : fullRel;
                const zipPath = `${prefix}/${rel}`.replaceAll('\\', '/');
                const entry = new ZipDeflate(zipPath);
                zip.add(entry);

                const isLfsPointer = await gitService.isLfsPointerAtCommit(fullRel, ref);
                const fileStream = isLfsPointer
                  ? gitService.getFileContentSmudgedStreamAtCommit(fullRel, ref)
                  : gitService.getFileContentStreamAtCommit(fullRel, ref);

                const reader = fileStream.getReader();
                try {
                  while (true) {
                    const { done, value } = await reader.read();
                    if (done) break;
                    if (value && value.byteLength > 0) {
                      entry.push(value.slice(0), false);
                    }
                  }
                } finally {
                  try {
                    await reader.cancel();
                  } catch {
                    // ignore
                  }
                  entry.push(new Uint8Array(0), true);
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
    }

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
