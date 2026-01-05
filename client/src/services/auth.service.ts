import { apiService } from "./api.service";
import type { ApiResponse } from "../types";

export interface AuthUser {
  id: string;
  username: string;
  role: string;
}

export interface AuthMeResponseEnabledFalse {
  enabled: false;
}

export interface AuthMeResponseEnabledTrue {
  user: AuthUser;
}

export type AuthMeResponse = AuthMeResponseEnabledFalse | AuthMeResponseEnabledTrue;

class AuthService {
  me(): Promise<ApiResponse<{ enabled: false } | { user: AuthUser }>> {
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
}

export const authService = new AuthService();
