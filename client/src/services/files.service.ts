import { apiService } from "./api.service";
import type { FileInfo, FileHistory } from "../types";

type DownloadProgress = { loaded: number; total?: number };

async function fetchToBlob(
  url: string,
  opts?: { signal?: AbortSignal; onProgress?: (p: DownloadProgress) => void },
): Promise<Blob> {
  const response = await fetch(url, { signal: opts?.signal });
  if (!response.ok) {
    throw new Error("下载失败");
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

export const filesService = {
  /**
   * 获取文件列表
   */
  async getFiles(path: string = "", commit?: string): Promise<FileInfo[]> {
    const response = await apiService.get<FileInfo[]>("/files", {
      path,
      commit,
    });
    return response.data || [];
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
    message: string = "创建目录",
  ): Promise<any> {
    return await apiService.post("/files/dir", { path, message });
  },

  /**
   * 获取文件内容
   */
  async getFileContent(path: string, commit?: string): Promise<Blob> {
    const params = new URLSearchParams({ path });
    if (commit) params.set("commit", commit);
    const response = await fetch(`/api/files/content?${params}`);

    if (!response.ok) {
      throw new Error("获取文件内容失败");
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
    },
  ): Promise<any> {
    async function fallbackSingleUpload() {
      const formData = new FormData();
      formData.append("file", file);
      formData.append("path", path);
      formData.append("message", message);

      if (opts?.signal || opts?.onProgress) {
        return await apiService.postFormWithProgress(
          "/files/upload",
          formData,
          {
            signal: opts?.signal,
            onUploadProgress: opts?.onProgress,
          },
        );
      }

      return await apiService.postForm("/files/upload", formData);
    }

    // 分块上传：默认启用（即使只有 1 块也可走同一流程），若后端不支持则回退
    try {
      const initResp = await apiService.post<{
        uploadId: string;
        chunkSize: number;
        totalChunks: number;
        received: number[];
        resumable: boolean;
      }>("/files/upload/init", {
        path,
        filename: file.name,
        size: file.size,
        lastModified: (file as any).lastModified ?? undefined,
        mime: file.type || undefined,
      });

      const initData = initResp.data;
      if (!initData?.uploadId || !initData.chunkSize || !initData.totalChunks) {
        // 兜底：若响应异常，回退旧上传
        return await fallbackSingleUpload();
      }

      const uploadId = initData.uploadId;
      const chunkSize = initData.chunkSize;
      const totalChunks = initData.totalChunks;
      const receivedSet = new Set<number>(
        (initData.received || []).filter((x) => Number.isFinite(x)),
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
        const buf = await slice.arrayBuffer();

        await apiService.postBinaryWithProgress("/files/upload/chunk", buf, {
          params: { uploadId, index },
          signal: opts?.signal,
          onUploadProgress: opts?.onProgress
            ? (p) => {
                // 当前块进度 + 已完成总量
                const base = uploadedBytes;
                const loaded = Math.min(totalBytes, base + (p.loaded ?? 0));
                opts.onProgress?.({ loaded, total: totalBytes });
              }
            : undefined,
        });

        uploadedBytes += bytesForIndex(index);
        opts?.onProgress?.({ loaded: uploadedBytes, total: totalBytes });
      }

      // 完成合并并提交
      const completeResp = await apiService.post("/files/upload/complete", {
        uploadId,
        message,
      });
      return completeResp;
    } catch (err: any) {
      // 若后端不支持分块端点（常见是 404）或协议异常，则自动回退旧上传
      const msg = err instanceof Error ? err.message : String(err ?? "");
      if (/404|not found/i.test(msg)) {
        return await fallbackSingleUpload();
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
   * 获取某个版本的 diff（unified diff 文本）
   */
  async getFileDiff(
    path: string,
    commit: string,
    parent?: string,
  ): Promise<string> {
    const params = new URLSearchParams({ path, commit });
    if (parent) params.set("parent", parent);
    const response = await fetch(`/api/history/diff?${params}`);
    if (!response.ok) {
      throw new Error("获取 diff 失败");
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
    const response = await apiService.get<FileInfo[]>("/search", {
      q: query,
      mode,
      type: opts?.type || "all",
      path: opts?.path || "",
    });
    return response.data || [];
  },

  /**
   * 创建分享链接
   */
  async createShareLink(
    path: string,
    opts?: { commit?: string; ttl?: number },
  ): Promise<{
    token: string;
    url: string;
    expiresIn: number;
    expiresAt: string;
  }> {
    const response = await apiService.post<{
      token: string;
      url: string;
      expiresIn: number;
      expiresAt: string;
    }>("/share", {
      path,
      commit: opts?.commit,
      ttl: opts?.ttl,
    });
    return response.data!;
  },

  /**
   * 获取分享链接信息
   */
  async getShareInfo(
    token: string,
  ): Promise<{ filePath: string; commit?: string; sharedBy?: string }> {
    const response = await apiService.get<{
      filePath: string;
      commit?: string;
      sharedBy?: string;
    }>("/share/info", { token });
    return response.data!;
  },
};
