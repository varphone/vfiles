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
  async searchFiles(query: string): Promise<FileInfo[]> {
    const response = await apiService.get<FileInfo[]>('/search', { q: query });
    return response.data || [];
  },
};
