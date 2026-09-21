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
  lastCommit?: { message?: string };
}

/** 收藏条目（`GET /api/files/favorites`）。 */
export interface FavoriteEntry {
  path: string;
  name: string;
  kind: "file" | "directory";
}

/** 侧栏聚合数据（`GET /api/files/overview`）。 */
/** 存储占用的分类：与后端 FileCategory 对应。 */
export type StorageCategory =
  | "document"
  | "image"
  | "video"
  | "audio"
  | "other";

export interface CategoryUsage {
  category: StorageCategory;
  bytes: number;
  file_count: number;
}

export interface WorkspaceOverview {
  file_count: number;
  directory_count: number;
  total_bytes: number;
  /** 按类型聚合的占用（按字节倒序）；服务端未提供时为空数组。 */
  categories: CategoryUsage[];
  recent_files: RecentFile[];
}

export interface RecentFile {
  path: string;
  name: string;
  size_bytes: number;
  mime_type?: string | null;
  updated_at: string;
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
