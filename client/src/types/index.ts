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

/** 访问令牌（GET /api/tokens；明文只在创建时返回一次）。 */
export interface AccessToken {
  id: string;
  name: string;
  token_prefix: string;
  scopes: string;
  created_at: string;
  expires_at: string | null;
  last_used_at: string | null;
  revoked_at: string | null;
  active: boolean;
}

export interface CreatedAccessToken {
  token: AccessToken;
  plaintext: string;
}

export interface TokenExpiryOption {
  days: number;
  label: string;
}

/** 所有权转移目标用户（GET /api/files/users/directory）。 */
export interface TransferTarget {
  id: string;
  username: string;
}

export interface TransferOwnershipResult {
  transferred: number;
  target_username: string;
}

/** 审计日志条目（GET /api/audit/logs，只读）。 */
export interface AuditLogEntry {
  id: string;
  created_at: string;
  user_id: string | null;
  username: string;
  action: string;
  result: "success" | "failure";
  target: string | null;
  ip: string | null;
  device: string | null;
  user_agent: string | null;
  detail: string | null;
}

export interface AuditLogPage {
  items: AuditLogEntry[];
  total: number;
  limit: number;
  offset: number;
}

export interface AuditCount {
  key: string;
  count: number;
}

export interface AuditLogSummary {
  total: number;
  failures: number;
  users: AuditCount[];
  actions: AuditCount[];
}

export interface AuditLogQueryParams {
  keyword?: string;
  action?: string;
  result?: "success" | "failure" | "";
  since?: string;
  until?: string;
  limit?: number;
  offset?: number;
}

/** 分享链接（GET /api/share/shares）。 */
export interface ShareLink {
  id: string;
  entry_id: string;
  entry_name: string;
  entry_path: string;
  entry_kind: "file" | "directory";
  code: string;
  expires_at: string | null;
  created_at: string;
  access_count: number;
  last_accessed_at: string | null;
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
  nextCursor?: string | null;
}

export interface ApiResponse<T = any> {
  success: boolean;
  data?: T;
  error?: string;
  message?: string;
}
