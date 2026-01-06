import { Hono } from "hono";
import { GitServiceManager } from "../services/git-service-manager.js";
import { pathSecurityMiddleware } from "../middleware/security.js";
import { config } from "../config.js";
import { validatePath } from "../utils/path-validator.js";
import { getRepoContext } from "../middleware/repo-context.js";
import fs from "node:fs/promises";
import nodePath from "node:path";
import crypto from "node:crypto";
import {
  isAllowedPathByPrefixes,
  normalizeRequestPath,
  validateRequiredString,
  validateOptionalCommitHash,
  validateOptionalString,
} from "../utils/validation.js";

/**
 * 生成符合 RFC 5987 的 Content-Disposition 头值
 */
function makeContentDisposition(
  type: "attachment" | "inline",
  filename: string,
): string {
  const hasNonAscii = /[^\x00-\x7F]/.test(filename);
  const needsQuotes = /["\\\s]/.test(filename);

  if (!hasNonAscii && !needsQuotes) {
    return `${type}; filename="${filename}"`;
  }

  const encoded = encodeURIComponent(filename).replace(/'/g, "%27");
  const asciiFallback = filename.replace(/[^\x20-\x7E]/g, "_");

  return `${type}; filename="${asciiFallback}"; filename*=UTF-8''${encoded}`;
}

export function createFilesRoutes(gitManager: GitServiceManager) {
  const app = new Hono();

  async function getGit(c: any) {
    const { repoPath, repoMode } = getRepoContext(c);
    const gitService = await gitManager.get(repoPath, repoMode);
    return { gitService, repoPath, repoMode };
  }

  function getFileExtension(filename: string): string {
    const lastDot = filename.lastIndexOf(".");
    if (lastDot <= 0 || lastDot === filename.length - 1) return "";
    return filename.slice(lastDot + 1).toLowerCase();
  }

  function isAllowedFileType(file: File): boolean {
    const allowed = config.allowedFileTypes;
    if (!allowed || allowed.length === 0) return true;

    const ext = getFileExtension(file.name);
    const mime = (file.type || "").toLowerCase();

    return allowed.some((rule) => {
      const normalized = rule.trim().toLowerCase();
      if (!normalized) return false;
      // mime 规则（如 image/png）
      if (normalized.includes("/")) return normalized === mime;
      // 扩展名规则（允许 "txt" 或 ".txt"）
      const normalizedExt = normalized.startsWith(".")
        ? normalized.slice(1)
        : normalized;
      return normalizedExt === ext;
    });
  }

  function isAllowedFileTypeByNameAndMime(
    filename: string,
    mime?: string,
  ): boolean {
    const allowed = config.allowedFileTypes;
    if (!allowed || allowed.length === 0) return true;

    const ext = getFileExtension(filename);
    const normalizedMime = (mime || "").toLowerCase();

    return allowed.some((rule) => {
      const normalized = rule.trim().toLowerCase();
      if (!normalized) return false;
      if (normalized.includes("/")) return normalized === normalizedMime;
      const normalizedExt = normalized.startsWith(".")
        ? normalized.slice(1)
        : normalized;
      return normalizedExt === ext;
    });
  }

  function isSafeUploadFilename(filename: string): boolean {
    // 只允许“文件名”本身，禁止携带路径分隔符/父目录跳转
    if (!filename) return false;
    if (filename.includes("/") || filename.includes("\\")) return false;
    if (filename === "." || filename === "..") return false;
    return true;
  }

  type UploadSession = {
    uploadId: string;
    filePath: string;
    filename: string;
    size: number;
    mime?: string;
    lastModified?: number;
    chunkSize: number;
    totalChunks: number;
    createdAt: number;
    updatedAt: number;
  };

  function getUploadSessionDir(uploadId: string): string {
    return nodePath.join(config.uploadTempDir, uploadId);
  }

  function getUploadSessionPath(uploadId: string): string {
    return nodePath.join(getUploadSessionDir(uploadId), "session.json");
  }

  function getChunkPath(uploadId: string, index: number): string {
    return nodePath.join(getUploadSessionDir(uploadId), `chunk_${index}.part`);
  }

  // 用于保护同一 uploadId 的并发操作
  const sessionLocks = new Map<string, Promise<unknown>>();

  function withSessionLock<T>(
    uploadId: string,
    fn: () => Promise<T>,
  ): Promise<T> {
    const prev = sessionLocks.get(uploadId) ?? Promise.resolve();
    const next = prev.then(fn, fn);
    sessionLocks.set(
      uploadId,
      next.then(
        () => undefined,
        () => undefined,
      ),
    );
    // 清理已完成的锁（避免内存泄漏）
    next.finally(() => {
      if (sessionLocks.get(uploadId) === next) {
        // 如果还是当前的，延迟清理
        setTimeout(() => {
          if (sessionLocks.get(uploadId) === next) {
            sessionLocks.delete(uploadId);
          }
        }, 5000);
      }
    });
    return next;
  }

  async function listReceivedChunks(
    uploadId: string,
    totalChunks: number,
  ): Promise<number[]> {
    // 并行检查所有分块是否存在
    const checks = Array.from({ length: totalChunks }, (_, i) =>
      fs
        .access(getChunkPath(uploadId, i))
        .then(() => i)
        .catch(() => -1),
    );
    const results = await Promise.all(checks);
    return results.filter((i) => i >= 0);
  }

  async function loadSession(uploadId: string): Promise<UploadSession | null> {
    try {
      const raw = await fs.readFile(getUploadSessionPath(uploadId), "utf-8");
      const parsed = JSON.parse(raw) as UploadSession;
      if (!parsed || typeof parsed !== "object") return null;
      if (!parsed.uploadId || !parsed.filePath) return null;
      return parsed;
    } catch {
      return null;
    }
  }

  async function saveSession(session: UploadSession): Promise<void> {
    await fs.mkdir(getUploadSessionDir(session.uploadId), { recursive: true });
    await fs.writeFile(
      getUploadSessionPath(session.uploadId),
      JSON.stringify(session),
      "utf-8",
    );
  }

  async function cleanupExpiredSessions(): Promise<void> {
    try {
      await fs.mkdir(config.uploadTempDir, { recursive: true });
      const entries = await fs.readdir(config.uploadTempDir, {
        withFileTypes: true,
      });
      const now = Date.now();
      for (const entry of entries) {
        if (!entry.isDirectory()) continue;
        const dir = nodePath.join(config.uploadTempDir, entry.name);
        const sessionPath = nodePath.join(dir, "session.json");
        try {
          const stat = await fs.stat(sessionPath);
          if (now - stat.mtimeMs > config.uploadSessionTtlMs) {
            await fs.rm(dir, { recursive: true, force: true });
          }
        } catch {
          // ignore
        }
      }
    } catch {
      // ignore
    }
  }

  function computeUploadId(input: {
    filePath: string;
    size: number;
    lastModified?: number;
  }): string {
    const h = crypto.createHash("sha256");
    h.update(input.filePath);
    h.update("|");
    h.update(String(input.size));
    h.update("|");
    h.update(String(input.lastModified ?? 0));
    return h.digest("hex");
  }

  /**
   * GET /api/files - 获取文件列表
   */
  app.get("/", pathSecurityMiddleware, async (c) => {
    const { gitService } = await getGit(c);
    const requestedPath = normalizeRequestPath(c.req.query("path") || "");
    const commit = c.req.query("commit");
    if (!isAllowedPathByPrefixes(requestedPath, config.allowedPathPrefixes)) {
      return c.json({ success: false, error: "不允许访问该路径" }, 403);
    }

    const commitResult = validateOptionalCommitHash(commit);
    if (!commitResult.ok) {
      return c.json(
        { success: false, error: commitResult.message },
        commitResult.status,
      );
    }

    const files = await gitService.listFiles(requestedPath, commitResult.value);

    return c.json({
      success: true,
      data: files,
    });
  });

  /**
   * GET /api/files/content - 获取文件内容
   */
  app.get("/content", pathSecurityMiddleware, async (c) => {
    const { gitService, repoPath, repoMode } = await getGit(c);
    const rawPath = c.req.query("path");
    const path = rawPath ? normalizeRequestPath(rawPath) : undefined;
    const commit = c.req.query("commit");

    if (!path) {
      return c.json(
        {
          success: false,
          error: "缺少path参数",
        },
        400,
      );
    }

    const commitResult = validateOptionalCommitHash(commit);
    if (!commitResult.ok) {
      return c.json(
        { success: false, error: commitResult.message },
        commitResult.status,
      );
    }

    try {
      // 设置响应头
      const filename = path.split("/").pop() || "file";
      c.header("Content-Type", "application/octet-stream");
      c.header(
        "Content-Disposition",
        makeContentDisposition("inline", filename),
      );

      // 当前版本：worktree 模式直接从磁盘流式输出；bare 模式从 HEAD 读取
      if (!commitResult.value && repoMode !== "bare") {
        const fullPath = nodePath.join(repoPath, path);
        const stat = await fs.stat(fullPath);
        if (!stat.isFile()) {
          return c.json({ success: false, error: "目标不是文件" }, 400);
        }

        c.header("Content-Length", stat.size.toString());
        const stream = new ReadableStream<Uint8Array>({
          start(controller) {
            (async () => {
              let file: fs.FileHandle | null = null;
              try {
                file = await fs.open(fullPath, "r");
                const buf = new Uint8Array(1024 * 1024);
                let offset = 0;
                while (offset < stat.size) {
                  const toRead = Math.min(buf.length, stat.size - offset);
                  const { bytesRead } = await file.read(buf, 0, toRead, offset);
                  if (bytesRead <= 0) break;
                  controller.enqueue(buf.slice(0, bytesRead));
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

      // 历史版本（commit）或 bare 模式当前版本：真正流式输出（git show stdout -> ReadableStream）
      const commit = commitResult.value || "HEAD";
      const exists = await gitService.fileExistsAtCommit(path, commit);
      if (!exists) {
        return c.json({ success: false, error: "读取文件失败" }, 404);
      }

      // Git LFS：若为 pointer，则 smudge 输出真实内容
      const isLfsPointer = await gitService.isLfsPointerAtCommit(path, commit);
      const stream = isLfsPointer
        ? gitService.getFileContentSmudgedStreamAtCommit(path, commit)
        : gitService.getFileContentStreamAtCommit(path, commit);

      // 仅在可知大小时设置 Content-Length（bare/commit 都可通过 cat-file -s 获取）
      try {
        const size = await gitService.getFileSizeAtCommit(path, commit);
        c.header("Content-Length", size.toString());
      } catch {
        // ignore
      }
      return c.body(stream);
    } catch (error) {
      return c.json(
        {
          success: false,
          error: error instanceof Error ? error.message : "读取文件失败",
        },
        404,
      );
    }
  });

  /**
   * POST /api/files/upload - 上传文件
   */
  app.post("/upload", async (c) => {
    try {
      const formData = await c.req.formData();
      const file = formData.get("file") as File;
      const rawPath = (formData.get("path") as string) || "";
      const requestedPath = normalizeRequestPath(rawPath);
      const messageResult = validateOptionalString(
        formData.get("message"),
        "message",
        { maxLength: 200 },
      );
      if (!messageResult.ok) {
        return c.json(
          { success: false, error: messageResult.message },
          messageResult.status,
        );
      }
      const message = messageResult.value || "上传文件";

      if (!file) {
        return c.json(
          {
            success: false,
            error: "没有文件",
          },
          400,
        );
      }

      // 文件名与路径安全校验
      if (!isSafeUploadFilename(file.name)) {
        return c.json(
          {
            success: false,
            error: "非法文件名",
          },
          400,
        );
      }

      // 文件大小限制（Bun File.size 为字节）
      if (file.size > config.maxFileSize) {
        return c.json(
          {
            success: false,
            error: `文件过大，最大允许 ${(config.maxFileSize / (1024 * 1024)).toFixed(0)}MB`,
          },
          413,
        );
      }

      // 文件类型/扩展名白名单
      if (!isAllowedFileType(file)) {
        return c.json(
          {
            success: false,
            error: "不允许的文件类型",
          },
          415,
        );
      }

      // 构建完整路径
      const filePath = requestedPath
        ? `${requestedPath}/${file.name}`
        : file.name;

      // 防止 path 穿越（确保最终写入路径在 repo 内）
      const { gitService, repoPath } = await getGit(c);
      if (!validatePath(filePath, repoPath)) {
        return c.json(
          {
            success: false,
            error: "无效的文件路径",
          },
          400,
        );
      }

      // 文件访问白名单（按最终文件路径判断）
      if (!isAllowedPathByPrefixes(filePath, config.allowedPathPrefixes)) {
        return c.json(
          {
            success: false,
            error: "不允许写入该路径",
          },
          403,
        );
      }

      // 保存文件（流式写入，避免把整个文件读入内存）
      const commitHash = await gitService.saveFile(filePath, file, message);

      return c.json({
        success: true,
        data: {
          path: filePath,
          commit: commitHash,
        },
        message: "文件上传成功",
      });
    } catch (error) {
      return c.json(
        {
          success: false,
          error: error instanceof Error ? error.message : "上传失败",
        },
        500,
      );
    }
  });

  /**
   * POST /api/files/upload/init - 分块上传初始化/续传探测
   */
  app.post("/upload/init", async (c) => {
    await cleanupExpiredSessions();

    try {
      const body = await c.req.json();

      const rawPath = typeof body?.path === "string" ? body.path : "";
      const requestedPath = normalizeRequestPath(rawPath);

      const filenameResult = validateRequiredString(
        body?.filename,
        "filename",
        { maxLength: 255 },
      );
      if (!filenameResult.ok) {
        return c.json(
          { success: false, error: filenameResult.message },
          filenameResult.status,
        );
      }
      const filename = filenameResult.value;

      const size = Number(body?.size);
      if (!Number.isFinite(size) || size < 0) {
        return c.json({ success: false, error: "无效的 size" }, 400);
      }
      if (size > config.maxFileSize) {
        return c.json(
          {
            success: false,
            error: `文件过大，最大允许 ${(config.maxFileSize / (1024 * 1024)).toFixed(0)}MB`,
          },
          413,
        );
      }

      const lastModified =
        body?.lastModified != null ? Number(body.lastModified) : undefined;
      const mime = typeof body?.mime === "string" ? body.mime : undefined;

      if (!isSafeUploadFilename(filename)) {
        return c.json({ success: false, error: "非法文件名" }, 400);
      }

      if (!isAllowedFileTypeByNameAndMime(filename, mime)) {
        return c.json({ success: false, error: "不允许的文件类型" }, 415);
      }

      const filePath = requestedPath
        ? `${requestedPath}/${filename}`
        : filename;

      const { repoPath } = getRepoContext(c);
      if (!validatePath(filePath, repoPath)) {
        return c.json({ success: false, error: "无效的文件路径" }, 400);
      }
      if (!isAllowedPathByPrefixes(filePath, config.allowedPathPrefixes)) {
        return c.json({ success: false, error: "不允许写入该路径" }, 403);
      }

      // chunkSize：由服务端下发，客户端必须遵守
      const chunkSize = Math.min(
        Math.max(1, config.uploadChunkSize),
        config.uploadMaxChunkSize,
      );
      const totalChunks = Math.max(1, Math.ceil(size / chunkSize));

      const uploadId = computeUploadId({ filePath, size, lastModified });
      const now = Date.now();

      const existing = await loadSession(uploadId);
      if (existing) {
        // 若参数不一致，视为新会话并清理旧分块
        if (
          existing.filePath !== filePath ||
          existing.size !== size ||
          existing.chunkSize !== chunkSize
        ) {
          await fs.rm(getUploadSessionDir(uploadId), {
            recursive: true,
            force: true,
          });
        }
      }

      const session: UploadSession = {
        uploadId,
        filePath,
        filename,
        size,
        mime,
        lastModified,
        chunkSize,
        totalChunks,
        createdAt: existing?.createdAt ?? now,
        updatedAt: now,
      };

      await saveSession(session);
      const received = await listReceivedChunks(uploadId, totalChunks);

      return c.json({
        success: true,
        data: {
          uploadId,
          chunkSize,
          totalChunks,
          received,
          resumable: received.length > 0,
        },
      });
    } catch (err) {
      return c.json(
        {
          success: false,
          error: err instanceof Error ? err.message : "初始化失败",
        },
        500,
      );
    }
  });

  /**
   * POST /api/files/upload/chunk?uploadId=...&index=...
   * Content-Type: application/octet-stream
   *
   * 使用流式写入，避免将整个分块加载到内存
   */
  app.post("/upload/chunk", async (c) => {
    const uploadId = c.req.query("uploadId") || "";
    const indexRaw = c.req.query("index");

    if (!uploadId || !/^[a-f0-9]{64}$/i.test(uploadId)) {
      return c.json({ success: false, error: "无效的 uploadId" }, 400);
    }

    const index = indexRaw != null ? Number.parseInt(indexRaw, 10) : NaN;
    if (!Number.isFinite(index) || index < 0) {
      return c.json({ success: false, error: "无效的 index" }, 400);
    }

    // 使用 session 锁保护并发操作
    return withSessionLock(uploadId, async () => {
      try {
        const session = await loadSession(uploadId);
        if (!session) {
          return c.json(
            { success: false, error: "上传会话不存在或已过期" },
            404,
          );
        }

        if (index >= session.totalChunks) {
          return c.json({ success: false, error: "index 超出范围" }, 400);
        }

        await fs.mkdir(getUploadSessionDir(uploadId), { recursive: true });

        // 流式写入分块文件，避免将整个分块加载到内存
        const chunkPath = getChunkPath(uploadId, index);
        const requestBody = c.req.raw.body;

        if (!requestBody) {
          return c.json({ success: false, error: "空分块" }, 400);
        }

        // 使用 Bun 的流式写入
        const bunFile = Bun.file(chunkPath);
        const writer = bunFile.writer();

        let size = 0;
        const reader = requestBody.getReader();
        try {
          while (true) {
            const { done, value } = await reader.read();
            if (done) break;
            if (value && value.byteLength > 0) {
              size += value.byteLength;
              if (size > config.uploadMaxChunkSize) {
                await writer.end();
                await fs.rm(chunkPath, { force: true });
                return c.json({ success: false, error: "分块过大" }, 413);
              }
              await writer.write(value);
            }
          }
        } finally {
          try {
            await reader.cancel();
          } catch {
            // ignore
          }
          await writer.end();
        }

        if (size === 0) {
          await fs.rm(chunkPath, { force: true });
          return c.json({ success: false, error: "空分块" }, 400);
        }

        session.updatedAt = Date.now();
        await saveSession(session);

        return c.json({ success: true, data: { uploadId, index } });
      } catch (err) {
        return c.json(
          {
            success: false,
            error: err instanceof Error ? err.message : "上传分块失败",
          },
          500,
        );
      }
    });
  });

  /**
   * POST /api/files/upload/complete - 合并分块并提交
   */
  app.post("/upload/complete", async (c) => {
    try {
      const body = await c.req.json();
      const uploadId = typeof body?.uploadId === "string" ? body.uploadId : "";

      const messageResult = validateOptionalString(body?.message, "message", {
        maxLength: 200,
      });
      if (!messageResult.ok) {
        return c.json(
          { success: false, error: messageResult.message },
          messageResult.status,
        );
      }
      const message = messageResult.value || "上传文件（分块）";

      if (!uploadId || !/^[a-f0-9]{64}$/i.test(uploadId)) {
        return c.json({ success: false, error: "无效的 uploadId" }, 400);
      }

      const session = await loadSession(uploadId);
      if (!session) {
        return c.json({ success: false, error: "上传会话不存在或已过期" }, 404);
      }

      const { gitService, repoPath, repoMode } = await getGit(c);

      // 再次做路径白名单校验（防止会话文件被篡改）
      if (!validatePath(session.filePath, repoPath)) {
        return c.json({ success: false, error: "无效的文件路径" }, 400);
      }
      if (
        !isAllowedPathByPrefixes(session.filePath, config.allowedPathPrefixes)
      ) {
        return c.json({ success: false, error: "不允许写入该路径" }, 403);
      }
      if (!isAllowedFileTypeByNameAndMime(session.filename, session.mime)) {
        return c.json({ success: false, error: "不允许的文件类型" }, 415);
      }

      // 使用并行检查缺失分块
      const missing = (
        await Promise.all(
          Array.from({ length: session.totalChunks }, (_, i) =>
            fs
              .access(getChunkPath(uploadId, i))
              .then(() => -1)
              .catch(() => i),
          ),
        )
      ).filter((i) => i >= 0);

      if (missing.length) {
        return c.json(
          {
            success: false,
            error: `缺少分块: ${missing.slice(0, 10).join(",")}${missing.length > 10 ? "..." : ""}`,
          },
          409,
        );
      }

      // 合并分块：使用流式读写，避免将分块全部加载到内存
      // 创建一个 ReadableStream 来流式读取所有分块
      const createChunksStream = (): ReadableStream<Uint8Array> => {
        let currentChunk = 0;
        let currentReader: ReadableStreamDefaultReader<Uint8Array> | null =
          null;

        return new ReadableStream<Uint8Array>({
          async pull(controller) {
            // 如果当前有正在读取的分块流
            if (currentReader) {
              const { done, value } = await currentReader.read();
              if (!done && value) {
                controller.enqueue(value);
                return;
              }
              // 当前分块读完了
              currentReader = null;
              currentChunk++;
            }

            // 检查是否还有更多分块
            if (currentChunk >= session.totalChunks) {
              controller.close();
              return;
            }

            // 打开下一个分块
            const chunkPath = getChunkPath(uploadId, currentChunk);
            const bunFile = Bun.file(chunkPath);
            const stream = bunFile.stream() as ReadableStream<Uint8Array>;
            currentReader = stream.getReader();

            // 读取第一块数据
            const { done, value } = await currentReader.read();
            if (!done && value) {
              controller.enqueue(value);
            } else {
              // 空分块，继续下一个
              currentReader = null;
              currentChunk++;
              // 递归调用 pull
              await this.pull!(controller);
            }
          },
          cancel() {
            if (currentReader) {
              currentReader.cancel().catch(() => {});
            }
          },
        });
      };

      let commitHash = "";
      if (repoMode === "bare") {
        // bare 模式：直接将流传给 saveFile
        const stream = createChunksStream();
        commitHash = await gitService.saveFile(
          session.filePath,
          stream,
          message,
        );
      } else {
        // worktree 模式：流式写入文件
        const fullPath = nodePath.join(repoPath, session.filePath);
        await fs.mkdir(nodePath.dirname(fullPath), { recursive: true });

        const bunFile = Bun.file(fullPath);
        const writer = bunFile.writer();
        const stream = createChunksStream();
        const reader = stream.getReader();

        try {
          while (true) {
            const { done, value } = await reader.read();
            if (done) break;
            if (value) {
              await writer.write(value);
            }
          }
        } finally {
          await reader.cancel().catch(() => {});
          await writer.end();
        }

        commitHash = await gitService.commitFile(session.filePath, message);
      }

      // 清理临时目录
      await fs.rm(getUploadSessionDir(uploadId), {
        recursive: true,
        force: true,
      });

      return c.json({
        success: true,
        data: {
          path: session.filePath,
          commit: commitHash,
        },
        message: "文件上传成功",
      });
    } catch (err) {
      return c.json(
        {
          success: false,
          error: err instanceof Error ? err.message : "合并失败",
        },
        500,
      );
    }
  });

  /**
   * DELETE /api/files - 删除文件
   */
  app.delete("/", pathSecurityMiddleware, async (c) => {
    const { gitService } = await getGit(c);
    const rawPath = c.req.query("path");
    const path = rawPath ? normalizeRequestPath(rawPath) : undefined;

    const messageResult = validateOptionalString(
      c.req.query("message"),
      "message",
      { maxLength: 200 },
    );
    if (!messageResult.ok) {
      return c.json(
        { success: false, error: messageResult.message },
        messageResult.status,
      );
    }
    const message = messageResult.value || "删除文件";

    if (!path) {
      return c.json(
        {
          success: false,
          error: "缺少path参数",
        },
        400,
      );
    }

    try {
      await gitService.deleteFile(path, message);

      return c.json({
        success: true,
        message: "文件删除成功",
      });
    } catch (error) {
      return c.json(
        {
          success: false,
          error: error instanceof Error ? error.message : "删除失败",
        },
        500,
      );
    }
  });

  /**
   * POST /api/files/move - 移动/重命名文件或目录
   */
  app.post("/move", async (c) => {
    const { gitService, repoPath } = await getGit(c);
    let body: any;
    try {
      body = await c.req.json();
    } catch {
      return c.json({ success: false, error: "请求体必须是 JSON" }, 400);
    }

    const fromResult = validateRequiredString(body?.from, "from", {
      minLength: 1,
      maxLength: 500,
    });
    if (!fromResult.ok)
      return c.json(
        { success: false, error: fromResult.message },
        fromResult.status,
      );

    const toResult = validateRequiredString(body?.to, "to", {
      minLength: 1,
      maxLength: 500,
    });
    if (!toResult.ok)
      return c.json(
        { success: false, error: toResult.message },
        toResult.status,
      );

    const messageResult = validateOptionalString(body?.message, "message", {
      maxLength: 200,
    });
    if (!messageResult.ok)
      return c.json(
        { success: false, error: messageResult.message },
        messageResult.status,
      );

    const fromPath = normalizeRequestPath(fromResult.value);
    const toPath = normalizeRequestPath(toResult.value);
    const message = messageResult.value || "移动/重命名";

    if (!fromPath || !toPath) {
      return c.json({ success: false, error: "from/to 不能为空" }, 400);
    }
    if (fromPath === toPath) {
      return c.json({ success: false, error: "from 和 to 不能相同" }, 400);
    }

    // 路径遍历防护（from/to 都必须在 repo 内）
    if (!validatePath(fromPath, repoPath) || !validatePath(toPath, repoPath)) {
      return c.json({ success: false, error: "无效的文件路径" }, 400);
    }

    // 文件访问白名单（from/to 都需满足）
    if (
      !isAllowedPathByPrefixes(fromPath, config.allowedPathPrefixes) ||
      !isAllowedPathByPrefixes(toPath, config.allowedPathPrefixes)
    ) {
      return c.json({ success: false, error: "不允许访问该路径" }, 403);
    }

    try {
      const commit = await gitService.movePath(fromPath, toPath, message);
      return c.json({
        success: true,
        data: { from: fromPath, to: toPath, commit },
        message: "移动成功",
      });
    } catch (error) {
      return c.json(
        {
          success: false,
          error: error instanceof Error ? error.message : "移动失败",
        },
        500,
      );
    }
  });

  /**
   * POST /api/files/dir - 创建目录
   */
  app.post("/dir", pathSecurityMiddleware, async (c) => {
    const { gitService, repoPath } = await getGit(c);
    let body: any;
    try {
      body = await c.req.json();
    } catch {
      return c.json({ success: false, error: "请求体必须是 JSON" }, 400);
    }

    const pathResult = validateRequiredString(body?.path, "path", {
      minLength: 1,
      maxLength: 500,
    });
    if (!pathResult.ok)
      return c.json(
        { success: false, error: pathResult.message },
        pathResult.status,
      );

    const messageResult = validateOptionalString(body?.message, "message", {
      maxLength: 200,
    });
    if (!messageResult.ok)
      return c.json(
        { success: false, error: messageResult.message },
        messageResult.status,
      );

    const dirPath = normalizeRequestPath(pathResult.value);
    const message = messageResult.value || `创建目录: ${dirPath}`;

    if (!dirPath) {
      return c.json({ success: false, error: "path 不能为空" }, 400);
    }

    if (!validatePath(dirPath, repoPath)) {
      return c.json({ success: false, error: "无效的文件路径" }, 400);
    }

    if (!isAllowedPathByPrefixes(dirPath, config.allowedPathPrefixes)) {
      return c.json({ success: false, error: "不允许访问该路径" }, 403);
    }

    try {
      const commit = await gitService.createDirectory(dirPath, message);
      return c.json({
        success: true,
        data: { path: dirPath, commit },
        message: "目录创建成功",
      });
    } catch (error) {
      const msg = error instanceof Error ? error.message : "创建失败";
      const status = /已存在/.test(msg) ? 409 : 500;
      return c.json({ success: false, error: msg }, status);
    }
  });

  return app;
}
