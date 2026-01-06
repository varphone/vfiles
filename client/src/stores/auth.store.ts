import { defineStore } from "pinia";
import { ref } from "vue";
import { authService, type AuthUser } from "../services/auth.service";

export const useAuthStore = defineStore("auth", () => {
  const initialized = ref(false);
  const enabled = ref<boolean>(false);
  const allowRegister = ref<boolean>(true);
  const user = ref<AuthUser | null>(null);
  const loading = ref(false);
  const error = ref<string | null>(null);

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
        error.value = res.error || "获取登录状态失败";
        return;
      }

      if ((res.data as any)?.enabled === false) {
        enabled.value = false;
        allowRegister.value = true;
        user.value = null;
        return;
      }

      enabled.value = true;
      allowRegister.value = Boolean((res.data as any)?.allowRegister);
      user.value = ((res.data as any)?.user as AuthUser | null) ?? null;
    } catch (e) {
      enabled.value = true;
      allowRegister.value = true;
      user.value = null;
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
      const res = await authService.login({ username, password });
      if (!res.success) {
        error.value = res.error || "登录失败";
        throw new Error(error.value);
      }
      enabled.value = true;
      user.value = (res.data as any)?.user ?? null;
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
      const res = await authService.register({ username, password, email });
      if (!res.success) {
        error.value = res.error || "注册失败";
        throw new Error(error.value);
      }
      enabled.value = true;
      user.value = (res.data as any)?.user ?? null;
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
      loading.value = false;
      initialized.value = true;
    }
  }

  function clearUser(): void {
    user.value = null;
  }

  return {
    initialized,
    enabled,
    allowRegister,
    user,
    loading,
    error,
    fetchMe,
    login,
    register,
    logout,
    clearUser,
  };
});
