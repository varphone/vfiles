import { apiService } from "./api.service";
import type { ApiResponse } from "../types";

export type UserRole = "admin" | "manager" | "user";

export interface AuthUser {
  id: string;
  username: string;
  email?: string;
  role: UserRole;
}

/**
 * 当前会话的权限范围。
 *
 * 主密码会话为全量权限（restricted=false）；受限密码会话只能访问 roots
 * 及其子目录，canWrite=false 时只读。
 */
export interface SessionScope {
  restricted: boolean;
  label: string | null;
  roots: string[];
  canWrite: boolean;
}

/** 受限登录密码（`GET /api/auth/credentials`，不含密码哈希）。 */
export interface LoginCredential {
  id: string;
  label: string;
  roots: string[];
  canWrite: boolean;
  createdAt: string;
  lastUsedAt: string | null;
}

export interface CreateLoginCredentialInput {
  label: string;
  password: string;
  roots: string[];
  canWrite?: boolean;
}

export interface UpdateLoginCredentialInput {
  label?: string;
  password?: string;
  roots?: string[];
  canWrite?: boolean;
}

export interface AdminUser {
  id: string;
  username: string;
  email?: string;
  role: UserRole;
  disabled?: boolean;
  createdAt: string;
  /** 最近一次会话活动时间；从未登录为 null。 */
  lastLogin?: string | null;
  /** 未撤销且未过期的会话数。 */
  activeSessions?: number;
}

export interface AdminUserCapabilities {
  canUpdateEmail: boolean;
  canRevokeSessions: boolean;
}

/** 管理台用户列表查询参数（服务端过滤/排序/分页）。 */
export interface AdminUsersQuery {
  q?: string;
  role?: UserRole | "";
  status?: "active" | "disabled" | "";
  page?: number;
  pageSize?: number;
  sort?: "created_at" | "username" | "role" | "last_login" | "status";
  order?: "asc" | "desc";
}

export interface AdminUsersPayload {
  users: AdminUser[];
  capabilities: AdminUserCapabilities;
  /** 后端分页响应含总数（r106 补型 ✓ 看板用户统计用） */
  total_count?: number;
  page?: number;
  page_size?: number;
}

/** 管理台用户抽屉里的会话记录。 */
export interface AdminUserSession {
  id: string;
  issuedAt: string;
  lastSeenAt: string;
  expiresAt: string;
  revokedAt: string | null;
  userAgent: string | null;
  ipAddr: string | null;
  credentialId: string | null;
  active: boolean;
}

type RustAdminUserSession = {
  id: string;
  issued_at?: string;
  issuedAt?: string;
  last_seen_at?: string;
  lastSeenAt?: string;
  expires_at?: string;
  expiresAt?: string;
  revoked_at?: string | null;
  revokedAt?: string | null;
  user_agent?: string | null;
  userAgent?: string | null;
  ip_addr?: string | null;
  ipAddr?: string | null;
  credential_id?: string | null;
  credentialId?: string | null;
  active?: boolean;
};

export interface SystemProtocolInfo {
  id: "http" | "webdav" | "ftps" | "s3" | "rsync";
  /** Configuration switch; runtime_status reports the observed startup result. */
  enabled: boolean;
  runtime_status: "unknown" | "disabled" | "started" | "failed";
  runtime_error: string | null;
  bind: string;
  embedded: boolean;
  mount_path: string | null;
  writable: boolean | null;
  module: string | null;
  passive_ports: string | null;
}

export interface SystemUserStorageUsage {
  user_id: string;
  username: string;
  role: UserRole;
  disabled: boolean;
  file_count: number;
  directory_count: number;
  total_bytes: number;
}

/** 管理台概览聚合（GET /api/admin/overview）。 */
export interface AdminOverviewPayload {
  userCount: number;
  activeUserCount: number;
  fileCount: number;
  directoryCount: number;
  totalBytes: number;
  activeSessions: number;
  failedLogins24h: number;
  version: string;
  startedAt: string;
  uptimeSecs: number;
}

type RustAdminOverview = {
  user_count?: number;
  active_user_count?: number;
  file_count?: number;
  directory_count?: number;
  total_bytes?: number;
  active_sessions?: number;
  failed_logins_24h?: number;
  version?: string;
  started_at?: string;
  uptime_secs?: number;
};

export interface SystemInfoPayload {
  version: string;
  os: string;
  arch: string;
  uptime_secs: number;
  started_at: string;
  webdav_enabled: boolean;
  webdav_bind: string;
  webdav_embedded: boolean;
  webdav_mount: string;
  protocols: SystemProtocolInfo[];
  storage: {
    user_count: number;
    file_count: number;
    directory_count: number;
    total_bytes: number;
    users: SystemUserStorageUsage[];
  };
}

export interface SessionFeatures {
  authEnabled: boolean;
  multiUser: boolean;
  emailLogin: boolean;
  searchContent: boolean;
  shareEnabled: boolean;
  historyEnabled: boolean;
  /** 是否启用 FTP 批量导入（决定上传对话框是否展示连接信息）。 */
  ftpEnabled: boolean;
  /** 单文件大小上限（字节）；0 或未提供表示未知。 */
  maxFileSizeBytes: number;
}

export interface SessionBootstrapPayload {
  authEnabled: boolean;
  currentUser: string | null;
  capabilities: string[];
  activeWorkspace: string | null;
  features: SessionFeatures;
}

type RustAdminUser = {
  id: string;
  username: string;
  email?: string;
  role: UserRole;
  disabled?: boolean;
  created_at?: string;
  createdAt?: string;
  last_login?: string | null;
  lastLogin?: string | null;
  active_sessions?: number;
  activeSessions?: number;
};

type RustAdminUsersResponse = {
  users: RustAdminUser[];
  total_count?: number;
  page?: number;
  page_size?: number;
};

type RustSessionFeatures = {
  auth_enabled?: boolean;
  authEnabled?: boolean;
  multi_user?: boolean;
  multiUser?: boolean;
  email_login?: boolean;
  emailLogin?: boolean;
  search_content?: boolean;
  searchContent?: boolean;
  share_enabled?: boolean;
  shareEnabled?: boolean;
  history_enabled?: boolean;
  historyEnabled?: boolean;
  ftp_enabled?: boolean;
  ftpEnabled?: boolean;
  /** 服务端 snake_case 字段。 */
  max_file_size_bytes?: number;
  maxFileSizeBytes?: number;
};

type RustSessionBootstrapResponse = {
  auth_enabled?: boolean;
  authEnabled?: boolean;
  current_user?: string | null;
  currentUser?: string | null;
  capabilities?: string[];
  active_workspace?: string | null;
  activeWorkspace?: string | null;
  features?: RustSessionFeatures;
};

/** 后端凭据 DTO 使用 camelCase；这里保留 snake_case 兜底以便兼容旧版本。 */
type RustLoginCredential = {
  id?: string;
  label?: string;
  roots?: string[];
  canWrite?: boolean;
  can_write?: boolean;
  createdAt?: string;
  created_at?: string;
  lastUsedAt?: string | null;
  last_used_at?: string | null;
};

const LEGACY_ADMIN_CAPABILITIES: AdminUserCapabilities = {
  canUpdateEmail: true,
  canRevokeSessions: true,
};

const RUST_ADMIN_CAPABILITIES: AdminUserCapabilities = {
  canUpdateEmail: true,
  canRevokeSessions: true,
};

function isWrappedApiResponse<T>(value: unknown): value is ApiResponse<T> {
  return (
    Boolean(value) &&
    typeof value === "object" &&
    "success" in (value as Record<string, unknown>)
  );
}

function isRustAdminUsersResponse(
  value: unknown,
): value is RustAdminUsersResponse {
  return (
    Boolean(value) &&
    typeof value === "object" &&
    Array.isArray((value as { users?: unknown }).users)
  );
}

function isRustSessionBootstrapResponse(
  value: unknown,
): value is RustSessionBootstrapResponse {
  return (
    Boolean(value) &&
    typeof value === "object" &&
    Array.isArray((value as { capabilities?: unknown }).capabilities)
  );
}

function isHttpStatus(error: unknown, status: number): boolean {
  return (
    Boolean(error) &&
    typeof error === "object" &&
    (error as { status?: number }).status === status
  );
}

function toSuccessResponse<T>(data?: T): ApiResponse<T> {
  return data === undefined ? { success: true } : { success: true, data };
}

function toErrorResponse<T>(error: string): ApiResponse<T> {
  return { success: false, error };
}

function normalizeAdminUser(user: RustAdminUser | AdminUser): AdminUser {
  const createdAt =
    user.createdAt ??
    ("created_at" in user && typeof user.created_at === "string"
      ? user.created_at
      : "");

  return {
    id: user.id,
    username: user.username,
    email: user.email,
    role: user.role,
    disabled: user.disabled,
    createdAt,
    lastLogin:
      user.lastLogin ??
      ("last_login" in user && typeof user.last_login === "string"
        ? user.last_login
        : null),
    activeSessions:
      user.activeSessions ??
      ("active_sessions" in user && typeof user.active_sessions === "number"
        ? user.active_sessions
        : 0),
  };
}

function normalizeWrappedAdminUsers(
  response: ApiResponse<{ users: AdminUser[] }>,
): ApiResponse<AdminUsersPayload> {
  if (!response.success) {
    return {
      success: false,
      error: response.error,
      message: response.message,
    };
  }

  return toSuccessResponse({
    users: (response.data?.users ?? []).map(normalizeAdminUser),
    capabilities: LEGACY_ADMIN_CAPABILITIES,
  });
}

function normalizeSessionFeatures(
  features?: RustSessionFeatures,
): SessionFeatures {
  return {
    authEnabled: Boolean(features?.authEnabled ?? features?.auth_enabled),
    multiUser: Boolean(features?.multiUser ?? features?.multi_user),
    emailLogin: Boolean(features?.emailLogin ?? features?.email_login),
    searchContent: Boolean(features?.searchContent ?? features?.search_content),
    shareEnabled: Boolean(features?.shareEnabled ?? features?.share_enabled),
    historyEnabled: Boolean(
      features?.historyEnabled ?? features?.history_enabled,
    ),
    ftpEnabled: Boolean(features?.ftpEnabled ?? features?.ftp_enabled),
    maxFileSizeBytes: Number(
      features?.maxFileSizeBytes ?? features?.max_file_size_bytes ?? 0,
    ),
  };
}

function normalizeSessionBootstrap(
  response: RustSessionBootstrapResponse,
): SessionBootstrapPayload {
  return {
    authEnabled: Boolean(response.authEnabled ?? response.auth_enabled),
    currentUser: response.currentUser ?? response.current_user ?? null,
    capabilities: response.capabilities ?? [],
    activeWorkspace:
      response.activeWorkspace ?? response.active_workspace ?? null,
    features: normalizeSessionFeatures(response.features),
  };
}

function normalizeLoginCredential(
  credential: RustLoginCredential,
): LoginCredential {
  return {
    id: String(credential.id ?? ""),
    label: String(credential.label ?? ""),
    roots: Array.isArray(credential.roots)
      ? credential.roots.filter(
          (root): root is string => typeof root === "string",
        )
      : [],
    canWrite: Boolean(credential.canWrite ?? credential.can_write),
    createdAt: String(credential.createdAt ?? credential.created_at ?? ""),
    lastUsedAt:
      credential.lastUsedAt ?? credential.last_used_at ?? null,
  };
}

/** 拆开 `{ success, data }` 信封；后端部分接口直接返回裸数据。 */
function unwrapPayload<T>(response: unknown): T | undefined {
  if (response && typeof response === "object" && "data" in response) {
    const wrapped = response as { data?: T; success?: boolean };
    if (wrapped.success !== false) return wrapped.data;
  }
  return response as T;
}

export interface AuthMeResponseEnabledFalse {
  enabled: false;
}

export interface AuthMeResponseEnabledTrue {
  enabled: true;
  allowRegister: boolean;
  user: AuthUser | null;
  scope: SessionScope | null;
}

export type AuthMeResponse =
  | AuthMeResponseEnabledFalse
  | AuthMeResponseEnabledTrue;

export interface LoginPayload {
  user: AuthUser;
  token?: string;
  scope?: SessionScope | null;
}

class AuthService {
  async getSessionBootstrap(): Promise<SessionBootstrapPayload | null> {
    const response = await apiService.get<unknown>("/session/bootstrap");
    if (!isRustSessionBootstrapResponse(response)) {
      return null;
    }
    return normalizeSessionBootstrap(response);
  }

  me(): Promise<ApiResponse<AuthMeResponse>> {
    return apiService.get("/auth/me");
  }

  login(input: {
    username: string;
    password: string;
  }): Promise<ApiResponse<LoginPayload>> {
    // Backend expects 'username_or_email' field
    const requestData = {
      username_or_email: input.username,
      password: input.password,
    };
    return apiService.post("/auth/login", requestData);
  }

  register(input: {
    username: string;
    password: string;
    email?: string;
  }): Promise<ApiResponse<{ user: AuthUser }>> {
    return apiService.post("/auth/register", input);
  }

  logout(): Promise<ApiResponse<void>> {
    return apiService.post("/auth/logout");
  }

  /** 更新个人邮箱（仅主密码会话）。 */
  updateProfile(email: string): Promise<ApiResponse<{ email: string }>> {
    return apiService.post("/auth/profile", { email });
  }

  /** 修改主密码；成功后服务端会撤销该账号的全部会话。 */
  changePassword(
    currentPassword: string,
    newPassword: string,
  ): Promise<ApiResponse<void>> {
    return apiService.post("/auth/password", {
      currentPassword,
      newPassword,
    });
  }

  // 受限登录密码（仅主密码会话可管理）
  /** 列出当前账号的受限登录密码（不含密码）。 */
  async listLoginCredentials(): Promise<LoginCredential[]> {
    const response = await apiService.get<unknown>("/auth/credentials");
    const payload = unwrapPayload<unknown>(response);
    if (!Array.isArray(payload)) return [];
    return payload.map((item) =>
      normalizeLoginCredential(item as RustLoginCredential),
    );
  }

  /** 新建一条受限登录密码。 */
  async createLoginCredential(
    input: CreateLoginCredentialInput,
  ): Promise<{ id: string }> {
    const response = await apiService.post<unknown>("/auth/credentials", {
      label: input.label,
      password: input.password,
      roots: input.roots,
      canWrite: input.canWrite === true,
    });
    const payload = unwrapPayload<{ id?: string }>(response);
    return { id: String(payload?.id ?? "") };
  }

  /**
   * 更新受限登录密码。
   *
   * 只发送调用方明确提供的字段；`password` 留空表示不修改密码。
   */
  async updateLoginCredential(
    id: string,
    input: UpdateLoginCredentialInput,
  ): Promise<{ updated: boolean }> {
    const body: Record<string, unknown> = {};
    if (input.label !== undefined) body.label = input.label;
    if (input.password !== undefined) body.password = input.password;
    if (input.roots !== undefined) body.roots = input.roots;
    if (input.canWrite !== undefined) body.canWrite = input.canWrite;

    const response = await apiService.post<unknown>(
      `/auth/credentials/${encodeURIComponent(id)}`,
      body,
    );
    const payload = unwrapPayload<{ updated?: boolean }>(response);
    return { updated: Boolean(payload?.updated ?? true) };
  }

  /** 删除受限登录密码；使用它的已有会话会立即失效。 */
  async deleteLoginCredential(id: string): Promise<void> {
    await apiService.delete(`/auth/credentials/${encodeURIComponent(id)}`);
  }

  // Admin APIs
  /** 重置用户密码（管理端 ✓ 后端 /users/:id/reset-password 备 ✓ r107' 补前端）。 */
  async resetUserPassword(
    encodedUserId: string,
    newPassword: string,
  ): Promise<ApiResponse<unknown>> {
    try {
      await apiService.post(`/admin/users/${encodedUserId}/reset-password`, {
        new_password: newPassword,
      });
      return { success: true };
    } catch (error) {
      return {
        success: false,
        error: error instanceof Error ? error.message : "重置密码失败",
      };
    }
  }

  /** 删除用户（管理端 ✓ 后端 DELETE /users/:id 备 ✓ r107' 补前端）。 */
  async deleteUser(encodedUserId: string): Promise<ApiResponse<unknown>> {
    try {
      await apiService.delete(`/admin/users/${encodedUserId}`);
      return { success: true };
    } catch (error) {
      return {
        success: false,
        error: error instanceof Error ? error.message : "删除用户失败",
      };
    }
  }

  /** 系统信息（管理端 ✓ r106 看板系统卡数据源）。 */
  async systemInfo(): Promise<ApiResponse<SystemInfoPayload>> {
    try {
      const response = await apiService.get<unknown>("/admin/system-info");
      return { success: true, data: response as never };
    } catch (error) {
      return {
        success: false,
        error: error instanceof Error ? error.message : "加载系统信息失败",
      };
    }
  }

  async listUsers(
    query: AdminUsersQuery = {},
  ): Promise<ApiResponse<AdminUsersPayload>> {
    try {
      const params: Record<string, unknown> = {};
      if (query.q) params.q = query.q;
      if (query.role) params.role = query.role;
      if (query.status) params.status = query.status;
      if (query.page) params.page = query.page;
      if (query.pageSize) params.page_size = query.pageSize;
      if (query.sort) params.sort = query.sort;
      if (query.order) params.order = query.order;
      const response = await apiService.get<unknown>(
        "/admin/users",
        Object.keys(params).length > 0 ? params : undefined,
      );

      if (isWrappedApiResponse<{ users: AdminUser[] }>(response)) {
        return normalizeWrappedAdminUsers(response);
      }

      if (isRustAdminUsersResponse(response)) {
        return toSuccessResponse({
          users: response.users.map(normalizeAdminUser),
          capabilities: RUST_ADMIN_CAPABILITIES,
          total_count: response.total_count,
          page: response.page,
          page_size: response.page_size,
        });
      }

      return toErrorResponse("加载失败");
    } catch (error) {
      if (!isHttpStatus(error, 404)) {
        throw error;
      }

      const response = await apiService.get<{ users: AdminUser[] }>(
        "/auth/users",
      );
      if (isWrappedApiResponse<{ users: AdminUser[] }>(response)) {
        return normalizeWrappedAdminUsers(response);
      }

      return toSuccessResponse({
        users: [],
        capabilities: LEGACY_ADMIN_CAPABILITIES,
      });
    }
  }

  /** 管理台概览：单次请求取回统计卡所需的聚合。 */
  async adminOverview(): Promise<AdminOverviewPayload | null> {
    const response = await apiService.get<unknown>("/admin/overview");
    const payload = ((response as any)?.data ?? response) as RustAdminOverview;
    if (!payload || typeof payload !== "object") return null;
    return {
      userCount: Number(payload.user_count ?? 0),
      activeUserCount: Number(payload.active_user_count ?? 0),
      fileCount: Number(payload.file_count ?? 0),
      directoryCount: Number(payload.directory_count ?? 0),
      totalBytes: Number(payload.total_bytes ?? 0),
      activeSessions: Number(payload.active_sessions ?? 0),
      failedLogins24h: Number(payload.failed_logins_24h ?? 0),
      version: String(payload.version ?? ""),
      startedAt: String(payload.started_at ?? ""),
      uptimeSecs: Number(payload.uptime_secs ?? 0),
    };
  }

  /** 管理台用户抽屉：该账号最近的会话。 */
  async listUserSessions(userId: string): Promise<AdminUserSession[]> {
    const encoded = encodeURIComponent(userId);
    const response = await apiService.get<unknown>(
      `/admin/users/${encoded}/sessions`,
    );
    const payload = Array.isArray(response)
      ? response
      : ((response as any)?.data ?? []);
    if (!Array.isArray(payload)) return [];
    return payload.map((raw: RustAdminUserSession) => ({
      id: String(raw.id ?? ""),
      issuedAt: raw.issuedAt ?? raw.issued_at ?? "",
      lastSeenAt: raw.lastSeenAt ?? raw.last_seen_at ?? "",
      expiresAt: raw.expiresAt ?? raw.expires_at ?? "",
      revokedAt: raw.revokedAt ?? raw.revoked_at ?? null,
      userAgent: raw.userAgent ?? raw.user_agent ?? null,
      ipAddr: raw.ipAddr ?? raw.ip_addr ?? null,
      credentialId: raw.credentialId ?? raw.credential_id ?? null,
      active: raw.active === true,
    }));
  }

  async setUserRole(
    userId: string,
    role: UserRole,
  ): Promise<ApiResponse<void>> {
    const encodedUserId = encodeURIComponent(userId);

    try {
      await apiService.put(`/admin/users/${encodedUserId}`, { role });
      return toSuccessResponse();
    } catch (error) {
      if (!isHttpStatus(error, 404)) {
        throw error;
      }
    }

    return apiService.post(`/auth/users/${encodedUserId}/role`, {
      role,
    });
  }

  async setUserDisabled(
    userId: string,
    disabled: boolean,
  ): Promise<ApiResponse<void>> {
    const encodedUserId = encodeURIComponent(userId);

    try {
      await apiService.put(`/admin/users/${encodedUserId}`, { disabled });
      return toSuccessResponse();
    } catch (error) {
      if (!isHttpStatus(error, 404)) {
        throw error;
      }
    }

    return apiService.post(`/auth/users/${encodedUserId}/disabled`, {
      disabled,
    });
  }

  async setUserEmail(
    userId: string,
    email: string,
  ): Promise<ApiResponse<void>> {
    const encodedUserId = encodeURIComponent(userId);

    try {
      await apiService.put(`/admin/users/${encodedUserId}`, { email });
      return toSuccessResponse();
    } catch (error) {
      if (!isHttpStatus(error, 404)) {
        throw error;
      }
    }

    return apiService.post(`/auth/users/${encodedUserId}/email`, {
      email,
    });
  }

  async revokeUserSessions(userId: string): Promise<ApiResponse<void>> {
    const encodedUserId = encodeURIComponent(userId);

    try {
      await apiService.post(`/admin/users/${encodedUserId}/revoke-sessions`);
      return toSuccessResponse();
    } catch (error) {
      if (!isHttpStatus(error, 404)) {
        throw error;
      }
    }

    return apiService.post(`/auth/users/${encodedUserId}/revoke-sessions`);
  }

  // v1.1.2: forgot password
  requestPasswordReset(email: string): Promise<ApiResponse<void>> {
    return apiService.post("/auth/password-reset/request", { email });
  }

  confirmPasswordReset(
    token: string,
    newPassword: string,
  ): Promise<ApiResponse<void>> {
    return apiService.post("/auth/password-reset/confirm", {
      token,
      newPassword,
    });
  }

  // v1.1.2: email code login
  requestEmailLoginCode(email: string): Promise<ApiResponse<void>> {
    return apiService.post("/auth/email-login/request", { email });
  }

  verifyEmailLoginCode(
    email: string,
    code: string,
  ): Promise<ApiResponse<{ user: AuthUser }>> {
    return apiService.post("/auth/email-login/verify", { email, code });
  }
}

export const authService = new AuthService();
