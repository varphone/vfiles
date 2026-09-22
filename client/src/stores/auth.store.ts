import { defineStore } from "pinia";
import { ref } from "vue";
import {
  authService,
  type AuthUser,
  type SessionBootstrapPayload,
  type SessionFeatures,
} from "../services/auth.service";

export const useAuthStore = defineStore("auth", () => {
  const initialized = ref(false);
  const enabled = ref<boolean>(false);
  const allowRegister = ref<boolean>(true);
  const user = ref<AuthUser | null>(null);
  const capabilities = ref<string[]>([]);
  const activeWorkspace = ref<string | null>(null);
  const features = ref<SessionFeatures | null>(null);
  const loading = ref(false);
  const error = ref<string | null>(null);

  function applySessionBootstrap(
    bootstrap: SessionBootstrapPayload | null,
  ): void {
    capabilities.value = bootstrap?.capabilities ?? [];
    activeWorkspace.value = bootstrap?.activeWorkspace ?? null;
    features.value = bootstrap?.features ?? null;
  }

  function clearSessionContext(): void {
    applySessionBootstrap(null);
  }

  async function syncSessionContext(): Promise<void> {
    try {
      const bootstrap = await authService.getSessionBootstrap();
      applySessionBootstrap(bootstrap);
    } catch {
      clearSessionContext();
    }
  }

  async function fetchMe(): Promise<void> {
    loading.value = true;
    error.value = null;
    try {
      const res = await authService.me();
      if (!res.success) {
        // server explicitly says not success
        enabled.value = true;
        allowRegister.value = true;
        user.value = null;
        clearSessionContext();
        error.value = res.error || "获取登录状态失败";
        return;
      }

      if ((res.data as any)?.enabled === false) {
        enabled.value = false;
        allowRegister.value = true;
        user.value = null;
        await syncSessionContext();
        return;
      }

      enabled.value = true;
      allowRegister.value = Boolean((res.data as any)?.allowRegister);
      user.value = ((res.data as any)?.user as AuthUser | null) ?? null;
      await syncSessionContext();
    } catch (e) {
      enabled.value = true;
      allowRegister.value = true;
      user.value = null;
      clearSessionContext();
      error.value = e instanceof Error ? e.message : "获取登录状态失败";
    } finally {
      initialized.value = true;
      loading.value = false;
    }
  }

  async function login(username: string, password: string): Promise<void> {
    loading.value = true;
    error.value = null;
    try {
      const loginData = await authService.login({ username, password });
      // New Rust API returns login data directly
      enabled.value = true;
      user.value = (loginData as any)?.user ?? null;
      // 标记登录成功时间，用于免疫期
      if (
        typeof window !== "undefined" &&
        (window as any).__vfiles_setLoginSuccess
      ) {
        (window as any).__vfiles_setLoginSuccess();
      }
    } catch (e) {
      error.value = e instanceof Error ? e.message : "登录失败";
      throw e;
    } finally {
      loading.value = false;
      initialized.value = true;
    }
  }

  async function register(
    username: string,
    password: string,
    email?: string,
  ): Promise<void> {
    loading.value = true;
    error.value = null;
    try {
      await authService.register({ username, password, email });
      // Registration creates the account but does not establish a session.
      // Log in immediately so the user lands in the application with a valid
      // auth cookie instead of looking logged-in until the next refresh.
      const loginData = await authService.login({ username, password });
      enabled.value = true;
      user.value = (loginData as any)?.user ?? null;
      // 标记登录成功时间，用于免疫期
      if (
        typeof window !== "undefined" &&
        (window as any).__vfiles_setLoginSuccess
      ) {
        (window as any).__vfiles_setLoginSuccess();
      }
    } catch (e) {
      error.value = e instanceof Error ? e.message : "注册失败";
      throw e;
    } finally {
      loading.value = false;
      initialized.value = true;
    }
  }

  async function logout(): Promise<void> {
    loading.value = true;
    error.value = null;
    try {
      await authService.logout();
    } finally {
      user.value = null;
      clearSessionContext();
      loading.value = false;
      initialized.value = true;
    }
  }

  function clearUser(): void {
    user.value = null;
    clearSessionContext();
  }

  return {
    initialized,
    enabled,
    allowRegister,
    user,
    capabilities,
    activeWorkspace,
    features,
    loading,
    error,
    fetchMe,
    syncSessionContext,
    login,
    register,
    logout,
    clearUser,
  };
});
