import { apiService } from "./api.service";
import { fetchWithRetry } from "./fetch-retry";
import type { ContentMatch, FileInfo, FileHistory } from "../types";

type DownloadProgress = { loaded: number; total?: number };

async function responseError(
  response: Response,
  fallback: string,
): Promise<Error> {
  if (response.status === 401 && typeof window !== "undefined") {
    window.dispatchEvent(new Event("vfiles:unauthorized"));
  }

  let message = fallback;
  try {
    const contentType = response.headers.get("content-type") || "";
    if (contentType.includes("application/json")) {
      const payload: unknown = await response.json();
      if (payload && typeof payload === "object") {
        const record = payload as Record<string, unknown>;
        if (typeof record.message === "string" && record.message) {
          message = record.message;
        } else if (typeof record.error === "string" && record.error) {
          message = record.error;
        }
      }
    } else {
      const text = (await response.text()).trim();
      if (text) message = text;
    }
  } catch {
    // Keep the caller-provided fallback when the error body cannot be parsed.
  }

  return new Error(message);
}

async function fetchToBlob(
  url: string,
  opts?: { signal?: AbortSignal; onProgress?: (p: DownloadProgress) => void },
): Promise<Blob> {
  // 只重试“拿到响应头之前”的失败，避免进度回退
  const response = await fetchWithRetry(url, {
    signal: opts?.signal,
    credentials: "include",
  });
  if (!response.ok) {
    throw await responseError(response, "下载失败");
  }

  const totalStr = response.headers.get("content-length");
  const total = totalStr ? Number.parseInt(totalStr, 10) : undefined;

  if (!response.body || !opts?.onProgress) {
    return response.blob();
  }

  const reader = response.body.getReader();
  const chunks: ArrayBuffer[] = [];
  let loaded = 0;

  while (true) {
    const { done, value } = await reader.read();
    if (done) break;
    if (!value) continue;
    // 复制到标准 ArrayBuffer，避免 ArrayBufferLike/SharedArrayBuffer 类型不兼容
    const buf = new ArrayBuffer(value.byteLength);
    new Uint8Array(buf).set(value);
    chunks.push(buf);
    loaded += value.byteLength;
    opts.onProgress({ loaded, total });
  }

  const mime =
    response.headers.get("content-type") || "application/octet-stream";
  return new Blob(chunks, { type: mime });
}

function triggerSaveBlob(blob: Blob, filename: string) {
  const objectUrl = URL.createObjectURL(blob);
  const link = document.createElement("a");
  link.href = objectUrl;
  link.download = filename;
  document.body.appendChild(link);
  link.click();
  document.body.removeChild(link);
  // 延迟释放，避免某些浏览器尚未读取完 objectURL
  setTimeout(() => URL.revokeObjectURL(objectUrl), 10_000);
}

function toIsoExpiry(ttlSeconds?: number): string | undefined {
  if (!ttlSeconds || ttlSeconds <= 0) return undefined;
  return new Date(Date.now() + ttlSeconds * 1000).toISOString();
}

function firefoxFileReadError(): Error {
  const error = new Error(
    "浏览器无法读取所选文件。Firefox 在某些目录或挂载点上可能会拒绝读取大文件，请将文件复制到本地临时目录后重试。",
  );
  error.name = "FirefoxFileReadError";
  return error;
}

function isFirefoxFileReadError(error: unknown): error is Error {
  return error instanceof Error && error.name === "FirefoxFileReadError";
}

async function readUploadChunk(
  blob: Blob,
  signal?: AbortSignal,
): Promise<ArrayBuffer> {
  if (typeof FileReader === "undefined") {
    return blob.arrayBuffer();
  }

  return await new Promise<ArrayBuffer>((resolve, reject) => {
    const reader = new FileReader();

    const cleanup = () => {
      signal?.removeEventListener("abort", handleAbort);
    };

    const failAsAbort = () => {
      cleanup();
      reject(new DOMException("Aborted", "AbortError"));
    };

    const handleAbort = () => {
      try {
        reader.abort();
      } catch {
        failAsAbort();
      }
    };

    if (signal?.aborted) {
      failAsAbort();
      return;
    }

    signal?.addEventListener("abort", handleAbort, { once: true });

    reader.onload = () => {
      cleanup();
      if (reader.result instanceof ArrayBuffer) {
        resolve(reader.result);
        return;
      }
      reject(new Error("读取上传文件失败"));
    };

    reader.onerror = () => {
      cleanup();
      if (signal?.aborted) {
        reject(new DOMException("Aborted", "AbortError"));
        return;
      }

      const name = reader.error?.name;
      if (
        name === "AbortError" ||
        name === "NotReadableError" ||
        name === "SecurityError"
      ) {
        reject(firefoxFileReadError());
        return;
      }

      reject(new Error(reader.error?.message || "读取上传文件失败"));
    };

    reader.onabort = () => {
      cleanup();
      if (signal?.aborted) {
        reject(new DOMException("Aborted", "AbortError"));
        return;
      }
      reject(firefoxFileReadError());
    };

    reader.readAsArrayBuffer(blob);
  });
}

type UploadInitResponse = {
  uploadId?: string;
  upload_id?: string;
  chunkSize?: number;
  chunk_size?: number;
  totalChunks?: number;
  total_chunks?: number;
  received?: number[];
  resumable?: boolean;
};

type SearchMatchDto = {
  match_type?: string;
  context?: string | null;
  line_number?: number | null;
};

type SearchVersionDto = {
  size_bytes?: number;
  mime_type?: string | null;
  is_text?: boolean;
  created_at?: string;
};

type SearchResultDto = {
  entry: FileInfo;
  version?: SearchVersionDto | null;
  matches?: SearchMatchDto[];
  score?: number;
};

function normalizeSearchScope(path?: string): string {
  return (path || "").trim().replace(/^\/+|\/+$/g, "");
}

function mergeSearchResults(results: SearchResultDto[]): SearchResultDto[] {
  const merged = new Map<string, SearchResultDto>();

  for (const result of results) {
    const key = result.entry.path;
    const existing = merged.get(key);
    if (!existing) {
      merged.set(key, {
        ...result,
        matches: [...(result.matches || [])],
      });
      continue;
    }

    existing.version = existing.version || result.version;
    existing.score = Math.max(existing.score || 0, result.score || 0);
    existing.matches = [...(existing.matches || []), ...(result.matches || [])];
  }

  return [...merged.values()];
}

function toContentMatches(matches?: SearchMatchDto[]): ContentMatch[] {
  const deduped = new Map<string, ContentMatch>();

  for (const match of matches || []) {
    if (match.match_type !== "content") continue;
    if (!match.context || !match.line_number || match.line_number <= 0)
      continue;

    const key = `${match.line_number}:${match.context}`;
    deduped.set(key, {
      line: match.line_number,
      text: match.context,
    });
  }

  return [...deduped.values()];
}

function mapSearchResultToFileInfo(result: SearchResultDto): FileInfo {
  const version = result.version || undefined;

  return {
    ...result.entry,
    size_bytes: version?.size_bytes ?? result.entry.size_bytes,
    mime_type: version?.mime_type ?? result.entry.mime_type,
    is_text: version?.is_text ?? result.entry.is_text,
    updated_at: version?.created_at ?? result.entry.updated_at,
    matches: toContentMatches(result.matches),
  };
}

export const filesService = {
  /**
   * 获取文件列表
   */
  async getFiles(path: string = "", commit?: string): Promise<FileInfo[]> {
    const endpoint = path
      ? `/files/tree/${encodeURIComponent(path)}`
      : "/files/tree";
    const search = new URLSearchParams();
    if (commit) search.set("commit", commit);
    const url = search.size > 0 ? `${endpoint}?${search.toString()}` : endpoint;
    const response = await apiService.get<FileInfo[]>(url);
    return Array.isArray(response) ? response : ((response as any)?.data ?? []);
  },

  /**
   * 分页获取目录列表（`GET /api/files/list`）。
   *
   * 返回 `total` 用于展示总数与判断是否还有更多；`getFiles` 仍保留给需要完整列表的
   * 场景（如移动对话框的重名检查）。
   */
  async getFilesPage(
    path: string = "",
    opts?: { commit?: string; limit?: number; offset?: number },
  ): Promise<{
    items: FileInfo[];
    total: number;
    limit: number;
    offset: number;
    has_more: boolean;
  }> {
    const endpoint = path
      ? `/files/list/${encodeURIComponent(path)}`
      : "/files/list";
    const search = new URLSearchParams();
    if (opts?.commit) search.set("commit", opts.commit);
    if (opts?.limit !== undefined) search.set("limit", String(opts.limit));
    if (opts?.offset !== undefined) search.set("offset", String(opts.offset));
    const url = search.size > 0 ? `${endpoint}?${search.toString()}` : endpoint;

    const response = await apiService.get<{
      items: FileInfo[];
      total: number;
      limit: number;
      offset: number;
      has_more: boolean;
    }>(url);
    const payload = (response as any)?.data ?? response;
    return {
      items: Array.isArray(payload?.items) ? payload.items : [],
      total: Number(payload?.total ?? 0),
      limit: Number(payload?.limit ?? 0),
      offset: Number(payload?.offset ?? 0),
      has_more: Boolean(payload?.has_more),
    };
  },

  /**
   * 移动/重命名文件或目录
   */
  async movePath(
    from: string,
    to: string,
    message: string = "移动/重命名",
  ): Promise<any> {
    return await apiService.post("/files/move", { from, to, message });
  },

  /**
   * 创建目录
   */
  async createDirectory(
    path: string,
    _message: string = "创建目录",
  ): Promise<any> {
    return await apiService.post("/files/directories", { path });
  },

  /**
   * 服务端缩略图地址。
   *
   * 直接返回 URL 交给 `<img>` 加载：同源请求会自动携带鉴权 Cookie，
   * 浏览器可以复用原生懒加载与 HTTP 缓存（服务端提供 ETag / Cache-Control），
   * 前端无需再手工管理 objectURL。
   */
  thumbnailUrl(
    path: string,
    opts?: { commit?: string; size?: number },
  ): string {
    const params = new URLSearchParams({ path });
    if (opts?.commit) params.set("commit", opts.commit);
    if (opts?.size) params.set("size", String(opts.size));
    return `/api/files/thumbnail?${params}`;
  },

  /**
   * 获取文件内容
   */
  async getFileContent(
    path: string,
    commit?: string,
    opts?: { signal?: AbortSignal },
  ): Promise<Blob> {
    const params = new URLSearchParams({ path });
    if (commit) params.set("commit", commit);
    const response = await fetchWithRetry(`/api/files/content?${params}`, {
      signal: opts?.signal,
      credentials: "include",
    });

    if (!response.ok) {
      throw await responseError(response, "获取文件内容失败");
    }

    return response.blob();
  },

  /**
   * 上传文件
   */
  async uploadFile(
    file: File,
    path: string = "",
    message: string = "上传文件",
    opts?: {
      signal?: AbortSignal;
      onProgress?: (p: { loaded: number; total?: number }) => void;
      relativePath?: string;
    },
  ): Promise<any> {
    // 计算目标路径：如果提供了 relativePath（来自目录选择），
    // 只取目录部分（不含文件名），因为后端会自动将文件名追加到路径
    const targetPath = opts?.relativePath
      ? (() => {
          const dirPath = opts.relativePath.substring(
            0,
            opts.relativePath.lastIndexOf("/"),
          );
          return `${path}/${dirPath}`.replace(/\/+/g, "/").replace(/^\//, "");
        })()
      : path;

    async function fallbackSingleUpload(mode: "xhr" | "native" = "xhr") {
      const formData = new FormData();
      formData.append("file", file);
      formData.append("path", targetPath);
      formData.append("message", message);

      if (mode === "native") {
        opts?.onProgress?.({ loaded: 0 });
        return await apiService.postFormNative("/files/upload", formData, {
          signal: opts?.signal,
        });
      }

      if (opts?.signal || opts?.onProgress) {
        return await apiService.postFormWithProgress(
          "/files/upload",
          formData,
          {
            signal: opts?.signal,
            timeoutMs: 0,
            onUploadProgress: opts?.onProgress,
          },
        );
      }

      return await apiService.postForm("/files/upload", formData, {
        signal: opts?.signal,
        timeoutMs: 0,
      });
    }

    // 分块上传：默认启用（即使只有 1 块也可走同一流程），若后端不支持则回退
    try {
      const initResp = await apiService.post<UploadInitResponse>(
        "/files/upload/init",
        {
          path: targetPath,
          filename: file.name,
          size: file.size,
          lastModified: (file as any).lastModified ?? undefined,
          mime: file.type || undefined,
        },
        {
          signal: opts?.signal,
          timeoutMs: 0,
        },
      );

      const initData = ((initResp as any)?.data ?? initResp) as
        | UploadInitResponse
        | undefined;
      const uploadId = initData?.uploadId ?? initData?.upload_id;
      const chunkSize = initData?.chunkSize ?? initData?.chunk_size;
      const totalChunks = initData?.totalChunks ?? initData?.total_chunks;

      if (!uploadId || !chunkSize || !totalChunks) {
        // 兜底：若响应异常，回退旧上传
        return await fallbackSingleUpload();
      }

      const receivedSet = new Set<number>(
        (initData?.received ?? []).filter((x) => Number.isFinite(x)),
      );

      const totalBytes = file.size;
      const bytesForIndex = (index: number) => {
        const start = index * chunkSize;
        const end = Math.min(totalBytes, start + chunkSize);
        return Math.max(0, end - start);
      };

      let alreadyBytes = 0;
      for (const idx of receivedSet) {
        if (idx >= 0 && idx < totalChunks) alreadyBytes += bytesForIndex(idx);
      }

      let uploadedBytes = alreadyBytes;
      opts?.onProgress?.({ loaded: uploadedBytes, total: totalBytes });

      for (let index = 0; index < totalChunks; index++) {
        if (opts?.signal?.aborted) {
          throw new DOMException("Aborted", "AbortError");
        }

        if (receivedSet.has(index)) {
          continue;
        }

        const start = index * chunkSize;
        const end = Math.min(totalBytes, start + chunkSize);
        const slice = file.slice(start, end);
        const buf = await readUploadChunk(slice, opts?.signal);

        await apiService.putBinaryWithProgress(
          `/files/upload/chunks/${encodeURIComponent(uploadId)}/${index}`,
          buf,
          {
            signal: opts?.signal,
            timeoutMs: 0,
            onUploadProgress: opts?.onProgress
              ? (p) => {
                  const base = uploadedBytes;
                  const loaded = Math.min(totalBytes, base + (p.loaded ?? 0));
                  opts.onProgress?.({ loaded, total: totalBytes });
                }
              : undefined,
          },
        );

        uploadedBytes += bytesForIndex(index);
        opts?.onProgress?.({ loaded: uploadedBytes, total: totalBytes });
      }

      // 完成合并并提交
      const completeResp = await apiService.post(
        `/files/upload/complete/${encodeURIComponent(uploadId)}`,
        {
          message,
        },
        {
          signal: opts?.signal,
          timeoutMs: 0,
        },
      );
      return completeResp;
    } catch (err: any) {
      // 若后端不支持分块端点（常见是 404）或协议异常，则自动回退旧上传
      const msg = err instanceof Error ? err.message : String(err ?? "");
      if (/404|not found/i.test(msg)) {
        return await fallbackSingleUpload();
      }
      if (isFirefoxFileReadError(err)) {
        try {
          return await fallbackSingleUpload("native");
        } catch (fallbackErr: any) {
          if (fallbackErr?.name === "AbortError") throw fallbackErr;
          if (
            fallbackErr instanceof Error &&
            /^(网络错误，请检查连接|请求失败)$/.test(fallbackErr.message)
          ) {
            throw err;
          }
          throw fallbackErr;
        }
      }
      // Abort 直接抛出
      if (err?.name === "AbortError") throw err;
      throw err;
    }
  },

  /**
   * 删除文件
   */
  async deleteFile(path: string, message: string = "删除文件"): Promise<any> {
    return await apiService.delete("/files", { path, message });
  },

  /**
   * 获取文件历史
   */
  async getFileHistory(path: string, limit: number = 50): Promise<FileHistory> {
    const response = await apiService.get<FileHistory>("/history", {
      path,
      limit,
    });
    return (
      response.data || { commits: [], currentVersion: "", totalCommits: 0 }
    );
  },

  /**
   * 还原某个历史版本为最新版本
   */
  async restoreFileVersion(
    path: string,
    commit: string,
    message?: string,
  ): Promise<any> {
    return await apiService.post("/history/restore", {
      path,
      commit,
      message,
    });
  },

  /**
   * 获取某个版本的 diff（unified diff 文本）
   */
  async getFileDiff(
    path: string,
    commit: string,
    parent?: string,
  ): Promise<string> {
    const params = new URLSearchParams({ path, commit });
    if (parent) params.set("parent", parent);
    const response = await fetchWithRetry(`/api/history/diff?${params}`, {
      credentials: "include",
    });
    if (!response.ok) {
      throw await responseError(response, "获取 diff 失败");
    }
    return response.text();
  },

  /**
   * 下载文件
   */
  downloadFile(path: string, commit?: string): void {
    const params = new URLSearchParams({ path });
    if (commit) params.set("commit", commit);
    const url = `/api/download?${params}`;
    const link = document.createElement("a");
    link.href = url;
    link.download = path.split("/").pop() || "download";
    document.body.appendChild(link);
    link.click();
    document.body.removeChild(link);
  },

  /**
   * 下载文件夹（ZIP）
   */
  downloadFolder(path: string, commit?: string): void {
    const params = { path };
    const qs = new URLSearchParams(params);
    if (commit) qs.set("commit", commit);
    const url = `/api/download/folder?${qs}`;
    const name = path.split("/").filter(Boolean).pop() || "root";
    const link = document.createElement("a");
    link.href = url;
    link.download = `${name}.zip`;
    document.body.appendChild(link);
    link.click();
    document.body.removeChild(link);
  },

  /**
   * 下载文件（返回 blob，支持进度/取消）
   */
  async fetchFileDownload(
    path: string,
    commit?: string,
    opts?: { signal?: AbortSignal; onProgress?: (p: DownloadProgress) => void },
  ): Promise<{ blob: Blob; filename: string }> {
    const params = new URLSearchParams({ path });
    if (commit) params.set("commit", commit);
    const url = `/api/download?${params}`;
    const filename = path.split("/").pop() || "download";
    const blob = await fetchToBlob(url, opts);
    return { blob, filename };
  },

  /**
   * 下载文件夹 ZIP（返回 blob，支持进度/取消；可能无 content-length）
   */
  async fetchFolderDownload(
    path: string,
    commit?: string,
    opts?: { signal?: AbortSignal; onProgress?: (p: DownloadProgress) => void },
  ): Promise<{ blob: Blob; filename: string }> {
    const params = new URLSearchParams({ path });
    if (commit) params.set("commit", commit);
    const url = `/api/download/folder?${params}`;
    const name = path.split("/").filter(Boolean).pop() || "root";
    const filename = `${name}.zip`;
    const blob = await fetchToBlob(url, opts);
    return { blob, filename };
  },

  /**
   * 保存下载结果
   */
  saveDownloadedBlob(blob: Blob, filename: string) {
    triggerSaveBlob(blob, filename);
  },

  /**
   * 搜索文件
   */
  async searchFiles(
    query: string,
    mode: "name" | "content" = "name",
    opts?: { type?: "all" | "file" | "directory"; path?: string },
  ): Promise<FileInfo[]> {
    const scope = normalizeSearchScope(opts?.path);
    const params: Record<string, string | number | boolean> = {
      q: query,
      search_files: true,
      search_content: mode === "content",
      limit: 500,
      offset: 0,
    };
    if (scope) {
      params.path = scope;
    }
    if (opts?.type && opts.type !== "all") {
      params.type = opts.type;
    }

    const response = await apiService.get<SearchResultDto[]>(
      "/files/search",
      params,
    );

    const payload = Array.isArray(response)
      ? response
      : (((response as any)?.data as SearchResultDto[] | undefined) ?? []);

    return mergeSearchResults(payload).map(mapSearchResultToFileInfo);
  },

  /**
   * 创建分享链接
   */
  async createShareLink(
    path: string,
    opts?: { commit?: string; ttl?: number },
  ): Promise<{
    code: string;
    url: string;
    expiresIn: number;
    expiresAt: string;
  }> {
    const expiresAt = toIsoExpiry(opts?.ttl);
    const response = (await apiService.post<{
      code: string;
      share_url: string;
    }>("/share/shares", {
      path,
      expires_at: expiresAt,
    })) as { code?: string; share_url?: string };

    if (!response?.code || !response?.share_url) {
      throw new Error("创建分享链接失败");
    }

    return {
      code: response.code,
      url: response.share_url,
      expiresIn: opts?.ttl ?? 0,
      expiresAt: expiresAt ?? "",
    };
  },

  /**
   * 获取分享链接信息
   */
  async getShareInfo(code: string): Promise<{
    filePath: string;
    commit?: string;
    sharedBy?: string;
    expiresAt: string;
  }> {
    const response = await apiService.get<{
      filePath: string;
      commit?: string;
      sharedBy?: string;
      expiresAt: string;
    }>("/share/info", { code });
    return response.data!;
  },
};
