import { apiService } from './api.service';
import type { FileInfo, FileHistory } from '../types';

type DownloadProgress = { loaded: number; total?: number };

async function fetchToBlob(
  url: string,
  opts?: { signal?: AbortSignal; onProgress?: (p: DownloadProgress) => void }
): Promise<Blob> {
  const response = await fetch(url, { signal: opts?.signal });
  if (!response.ok) {
    throw new Error('下载失败');
  }

  const totalStr = response.headers.get('content-length');
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

  const mime = response.headers.get('content-type') || 'application/octet-stream';
  return new Blob(chunks, { type: mime });
}

function triggerSaveBlob(blob: Blob, filename: string) {
  const objectUrl = URL.createObjectURL(blob);
  const link = document.createElement('a');
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
  async getFiles(path: string = ''): Promise<FileInfo[]> {
    const response = await apiService.get<FileInfo[]>('/files', { path });
    return response.data || [];
  },


  /**
   * 移动/重命名文件或目录
   */
  async movePath(from: string, to: string, message: string = '移动/重命名'): Promise<any> {
    return await apiService.post('/files/move', { from, to, message });
  },

  /**
   * 获取文件内容
   */
  async getFileContent(path: string, commit?: string): Promise<Blob> {
    const params = new URLSearchParams({ path });
    if (commit) params.set('commit', commit);
    const response = await fetch(`/api/files/content?${params}`);
    
    if (!response.ok) {
      throw new Error('获取文件内容失败');
    }
    
    return response.blob();
  },

  /**
   * 上传文件
   */
  async uploadFile(file: File, path: string = '', message: string = '上传文件'): Promise<any> {
    const formData = new FormData();
    formData.append('file', file);
    formData.append('path', path);
    formData.append('message', message);

    return await apiService.postForm('/files/upload', formData);
  },

  /**
   * 删除文件
   */
  async deleteFile(path: string, message: string = '删除文件'): Promise<any> {
    return await apiService.delete('/files', { path, message });
  },

  /**
   * 获取文件历史
   */
  async getFileHistory(path: string, limit: number = 50): Promise<FileHistory> {
    const response = await apiService.get<FileHistory>('/history', { path, limit });
    return response.data || { commits: [], currentVersion: '', totalCommits: 0 };
  },

  /**
   * 获取某个版本的 diff（unified diff 文本）
   */
  async getFileDiff(path: string, commit: string, parent?: string): Promise<string> {
    const params = new URLSearchParams({ path, commit });
    if (parent) params.set('parent', parent);
    const response = await fetch(`/api/history/diff?${params}`);
    if (!response.ok) {
      throw new Error('获取 diff 失败');
    }
    return response.text();
  },

  /**
   * 下载文件
   */
  downloadFile(path: string, commit?: string): void {
    const params = new URLSearchParams({ path });
    if (commit) params.set('commit', commit);
    const url = `/api/download?${params}`;
    const link = document.createElement('a');
    link.href = url;
    link.download = path.split('/').pop() || 'download';
    document.body.appendChild(link);
    link.click();
    document.body.removeChild(link);
  },

  /**
   * 下载文件夹（ZIP）
   */
  downloadFolder(path: string): void {
    const params = { path };
    const url = `/api/download/folder?${new URLSearchParams(params)}`;
    const name = path.split('/').filter(Boolean).pop() || 'root';
    const link = document.createElement('a');
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
    opts?: { signal?: AbortSignal; onProgress?: (p: DownloadProgress) => void }
  ): Promise<{ blob: Blob; filename: string }> {
    const params = new URLSearchParams({ path });
    if (commit) params.set('commit', commit);
    const url = `/api/download?${params}`;
    const filename = path.split('/').pop() || 'download';
    const blob = await fetchToBlob(url, opts);
    return { blob, filename };
  },

  /**
   * 下载文件夹 ZIP（返回 blob，支持进度/取消；可能无 content-length）
   */
  async fetchFolderDownload(
    path: string,
    opts?: { signal?: AbortSignal; onProgress?: (p: DownloadProgress) => void }
  ): Promise<{ blob: Blob; filename: string }> {
    const params = new URLSearchParams({ path });
    const url = `/api/download/folder?${params}`;
    const name = path.split('/').filter(Boolean).pop() || 'root';
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
    mode: 'name' | 'content' = 'name',
    opts?: { type?: 'all' | 'file' | 'directory'; path?: string }
  ): Promise<FileInfo[]> {
    const response = await apiService.get<FileInfo[]>('/search', {
      q: query,
      mode,
      type: opts?.type || 'all',
      path: opts?.path || '',
    });
    return response.data || [];
  },
};
