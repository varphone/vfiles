// 类型定义
export interface FileInfo {
  id: string;
  name: string;
  path: string;
  kind: "file" | "directory";
  size_bytes?: number;
  mime_type?: string;
  is_text?: boolean;
  created_at: string;
  updated_at?: string;
  matches?: ContentMatch[];
}

export interface ContentMatch {
  line: number;
  text: string;
}

export interface CommitInfo {
  hash: string;
  message: string;
  changeType?: "added" | "modified" | "deleted" | "renamed";
  hasCustomMessage?: boolean;
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
