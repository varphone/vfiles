// 类型定义
export interface FileInfo {
  name: string;
  path: string;
  type: 'file' | 'directory';
  size: number;
  mtime: string;
  lastCommit?: CommitSummary;
}

export interface CommitInfo {
  hash: string;
  message: string;
  author: AuthorInfo;
  date: string;
  parent: string[];
}

export interface CommitSummary {
  hash: string;
  message: string;
  author: string;
  date: string;
}

export interface AuthorInfo {
  name: string;
  email: string;
}

export interface FileHistory {
  commits: CommitInfo[];
  currentVersion: string;
  totalCommits: number;
}

export interface ApiResponse<T = any> {
  success: boolean;
  data?: T;
  error?: string;
  message?: string;
}
