import { apiService } from './api.service';
import type { FileInfo, FileHistory } from '../types';

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
    const params = commit ? { path, commit } : { path };
    const response = await fetch(`/api/files/content?${new URLSearchParams(params)}`);
    
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
    const params = parent ? { path, commit, parent } : { path, commit };
    const response = await fetch(`/api/history/diff?${new URLSearchParams(params)}`);
    if (!response.ok) {
      throw new Error('获取 diff 失败');
    }
    return response.text();
  },

  /**
   * 下载文件
   */
  downloadFile(path: string, commit?: string): void {
    const params = commit ? { path, commit } : { path };
    const url = `/api/download?${new URLSearchParams(params)}`;
    const link = document.createElement('a');
    link.href = url;
    link.download = path.split('/').pop() || 'download';
    document.body.appendChild(link);
    link.click();
    document.body.removeChild(link);
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
