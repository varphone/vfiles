import { apiService } from "./api.service";
import type { ApiResponse } from "../types";

export interface AuthUser {
  id: string;
  username: string;
  role: string;
}

export interface AdminUser {
  id: string;
  username: string;
  role: string;
  disabled?: boolean;
  createdAt: string;
}

export interface AuthMeResponseEnabledFalse {
  enabled: false;
}

export interface AuthMeResponseEnabledTrue {
  user: AuthUser;
}

export type AuthMeResponse = AuthMeResponseEnabledFalse | AuthMeResponseEnabledTrue;

class AuthService {
  me(): Promise<
    ApiResponse<
      | { enabled: false }
      | { enabled: true; allowRegister: boolean; user: AuthUser | null }
    >
  > {
    return apiService.get("/auth/me");
  }

  login(input: { username: string; password: string }): Promise<ApiResponse<{ user: AuthUser }>> {
    return apiService.post("/auth/login", input);
  }

  register(input: { username: string; password: string }): Promise<ApiResponse<{ user: AuthUser }>> {
    return apiService.post("/auth/register", input);
  }

  logout(): Promise<ApiResponse<void>> {
    return apiService.post("/auth/logout");
  }

  // Admin APIs
  listUsers(): Promise<ApiResponse<{ users: AdminUser[] }>> {
    return apiService.get("/auth/users");
  }

  setUserRole(userId: string, role: "admin" | "user"): Promise<ApiResponse<void>> {
    return apiService.post(`/auth/users/${encodeURIComponent(userId)}/role`, {
      role,
    });
  }

  setUserDisabled(userId: string, disabled: boolean): Promise<ApiResponse<void>> {
    return apiService.post(
      `/auth/users/${encodeURIComponent(userId)}/disabled`,
      {
        disabled,
      },
    );
  }

  revokeUserSessions(userId: string): Promise<ApiResponse<void>> {
    return apiService.post(
      `/auth/users/${encodeURIComponent(userId)}/revoke-sessions`,
    );
  }
}

export const authService = new AuthService();
