import { apiService } from "./api.service";
import type { ApiResponse } from "../types";

export type UserRole = "admin" | "manager" | "user";

export interface AuthUser {
  id: string;
  username: string;
  email?: string;
  role: UserRole;
}

export interface AdminUser {
  id: string;
  username: string;
  email?: string;
  role: UserRole;
  disabled?: boolean;
  createdAt: string;
}

export interface AdminUserCapabilities {
  canUpdateEmail: boolean;
  canRevokeSessions: boolean;
}

export interface AdminUsersPayload {
  users: AdminUser[];
  capabilities: AdminUserCapabilities;
  /** 后端分页响应含总数（r106 补型 ✓ 看板用户统计用） */
  total_count?: number;
  page?: number;
  page_size?: number;
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

export interface AuthMeResponseEnabledFalse {
  enabled: false;
}

export interface AuthMeResponseEnabledTrue {
  user: AuthUser;
}

export type AuthMeResponse =
  | AuthMeResponseEnabledFalse
  | AuthMeResponseEnabledTrue;

class AuthService {
  async getSessionBootstrap(): Promise<SessionBootstrapPayload | null> {
    const response = await apiService.get<unknown>("/session/bootstrap");
    if (!isRustSessionBootstrapResponse(response)) {
      return null;
    }
    return normalizeSessionBootstrap(response);
  }

  me(): Promise<
    ApiResponse<
      | { enabled: false }
      | { enabled: true; allowRegister: boolean; user: AuthUser | null }
    >
  > {
    return apiService.get("/auth/me");
  }

  login(input: {
    username: string;
    password: string;
  }): Promise<ApiResponse<{ user: AuthUser }>> {
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
  async systemInfo(): Promise<
    ApiResponse<{
      version: string;
      os: string;
      arch: string;
      uptime_secs: number;
      started_at: string;
      webdav_enabled: boolean;
      webdav_bind: string;
    }>
  > {
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

  async listUsers(): Promise<ApiResponse<AdminUsersPayload>> {
    try {
      const response = await apiService.get<unknown>("/admin/users");

      if (isWrappedApiResponse<{ users: AdminUser[] }>(response)) {
        return normalizeWrappedAdminUsers(response);
      }

      if (isRustAdminUsersResponse(response)) {
        return toSuccessResponse({
          users: response.users.map(normalizeAdminUser),
          capabilities: RUST_ADMIN_CAPABILITIES,
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
