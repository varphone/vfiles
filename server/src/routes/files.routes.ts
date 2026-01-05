import { Hono } from "hono";
import { GitService } from "../services/git.service.js";
import { pathSecurityMiddleware } from "../middleware/security.js";
import { config } from "../config.js";
import { validatePath } from "../utils/path-validator.js";
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

export function createFilesRoutes(gitService: GitService) {
  const app = new Hono();

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

  async function listReceivedChunks(
    uploadId: string,
    totalChunks: number,
  ): Promise<number[]> {
    const received: number[] = [];
    for (let i = 0; i < totalChunks; i++) {
      try {
        await fs.access(getChunkPath(uploadId, i));
        received.push(i);
      } catch {
        // not received
      }
    }
    return received;
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
      c.header("Content-Type", "application/octet-stream");
      c.header(
        "Content-Disposition",
        `inline; filename="${path.split("/").pop()}"`,
      );

      // 当前版本：worktree 模式直接从磁盘流式输出；bare 模式从 HEAD 读取
      if (!commitResult.value && config.repoMode !== "bare") {
        const fullPath = nodePath.join(config.repoPath, path);
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
      if (!validatePath(filePath, config.repoPath)) {
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

      if (!validatePath(filePath, config.repoPath)) {
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
   */
  app.post("/upload/chunk", async (c) => {
    try {
      const uploadId = c.req.query("uploadId") || "";
      const indexRaw = c.req.query("index");

      if (!uploadId || !/^[a-f0-9]{64}$/i.test(uploadId)) {
        return c.json({ success: false, error: "无效的 uploadId" }, 400);
      }

      const index = indexRaw != null ? Number.parseInt(indexRaw, 10) : NaN;
      if (!Number.isFinite(index) || index < 0) {
        return c.json({ success: false, error: "无效的 index" }, 400);
      }

      const session = await loadSession(uploadId);
      if (!session) {
        return c.json({ success: false, error: "上传会话不存在或已过期" }, 404);
      }

      if (index >= session.totalChunks) {
        return c.json({ success: false, error: "index 超出范围" }, 400);
      }

      const buf = Buffer.from(await c.req.arrayBuffer());
      if (buf.length <= 0) {
        return c.json({ success: false, error: "空分块" }, 400);
      }
      if (buf.length > config.uploadMaxChunkSize) {
        return c.json({ success: false, error: "分块过大" }, 413);
      }

      await fs.mkdir(getUploadSessionDir(uploadId), { recursive: true });
      await fs.writeFile(getChunkPath(uploadId, index), buf);

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

      // 再次做路径白名单校验（防止会话文件被篡改）
      if (!validatePath(session.filePath, config.repoPath)) {
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

      const missing: number[] = [];
      for (let i = 0; i < session.totalChunks; i++) {
        try {
          await fs.access(getChunkPath(uploadId, i));
        } catch {
          missing.push(i);
        }
      }

      if (missing.length) {
        return c.json(
          {
            success: false,
            error: `缺少分块: ${missing.slice(0, 10).join(",")}${missing.length > 10 ? "..." : ""}`,
          },
          409,
        );
      }

      // 合并分块：worktree 模式写入 repo 工作区后 commitFile；bare 模式合并到临时文件后 saveFile 直接写入对象库
      let commitHash = "";
      if (config.repoMode === "bare") {
        const mergedPath = nodePath.join(
          getUploadSessionDir(uploadId),
          "merged.bin",
        );
        const handle = await fs.open(mergedPath, "w");
        try {
          for (let i = 0; i < session.totalChunks; i++) {
            const chunkBuf = await fs.readFile(getChunkPath(uploadId, i));
            await handle.write(chunkBuf);
          }
        } finally {
          await handle.close();
        }

        const stream = Bun.file(
          mergedPath,
        ).stream() as ReadableStream<Uint8Array>;
        commitHash = await gitService.saveFile(
          session.filePath,
          stream,
          message,
        );
      } else {
        const fullPath = nodePath.join(config.repoPath, session.filePath);
        await fs.mkdir(nodePath.dirname(fullPath), { recursive: true });

        const handle = await fs.open(fullPath, "w");
        try {
          for (let i = 0; i < session.totalChunks; i++) {
            const chunkBuf = await fs.readFile(getChunkPath(uploadId, i));
            await handle.write(chunkBuf);
          }
        } finally {
          await handle.close();
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
    if (
      !validatePath(fromPath, config.repoPath) ||
      !validatePath(toPath, config.repoPath)
    ) {
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

    if (!validatePath(dirPath, config.repoPath)) {
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
