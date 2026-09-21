<template>
  <AuthShell>
    <AuthNotice v-if="auth.initialized && auth.enabled === false" tone="info">
      当前服务未启用认证（ENABLE_AUTH=false）。
      <button class="auth-link-button" type="button" @click="goHome">
        返回首页
      </button>
    </AuthNotice>

    <template v-else>
      <AuthNotice v-if="reasonText" tone="warning">{{ reasonText }}</AuthNotice>

      <!-- 模式切换：登录 / 邮箱验证码 / 注册（分段控件，与主界面控件语言一致） -->
      <div class="auth-modes" role="tablist" aria-label="登录方式">
        <button
          v-for="option in modeOptions"
          :key="option.value"
          class="auth-mode"
          :class="{ 'is-active': mode === option.value }"
          type="button"
          role="tab"
          :aria-selected="mode === option.value ? 'true' : 'false'"
          @click="mode = option.value"
        >
          {{ option.label }}
        </button>
      </div>

      <AuthNotice
        v-if="auth.enabled && !auth.allowRegister && mode !== 'email'"
        tone="info"
      >
        管理员已关闭注册（AUTH_ALLOW_REGISTER=false）。
      </AuthNotice>

      <form v-if="mode !== 'email'" class="auth-form" @submit.prevent="submit">
        <div class="auth-field">
          <label class="auth-label" for="auth-username">用户名</label>
          <div class="auth-input-wrap">
            <IconUser :size="16" class="auth-input-icon" />
            <input
              id="auth-username"
              v-model.trim="username"
              class="input auth-input"
              type="text"
              autocomplete="username"
              placeholder="3-32 位，字母数字-_"
              :disabled="auth.loading"
            />
          </div>
        </div>

        <div v-if="mode === 'register'" class="auth-field">
          <label class="auth-label" for="auth-email">邮箱（可选）</label>
          <div class="auth-input-wrap">
            <IconMail :size="16" class="auth-input-icon" />
            <input
              id="auth-email"
              v-model.trim="email"
              class="input auth-input"
              type="email"
              autocomplete="email"
              placeholder="用于找回密码/验证码登录"
              :disabled="auth.loading"
            />
          </div>
        </div>

        <div class="auth-field">
          <label class="auth-label" for="auth-password">密码</label>
          <div class="auth-input-wrap">
            <IconLock :size="16" class="auth-input-icon" />
            <input
              id="auth-password"
              v-model="password"
              class="input auth-input"
              type="password"
              :autocomplete="
                mode === 'login' ? 'current-password' : 'new-password'
              "
              placeholder="至少 6 位"
              :disabled="auth.loading"
            />
          </div>
        </div>

        <button
          class="vf-ghost-button is-primary auth-submit"
          :class="{ 'is-loading': auth.loading }"
          :disabled="auth.loading"
          type="submit"
        >
          {{ mode === "login" ? "登录" : "注册" }}
        </button>

        <div v-if="mode === 'login'" class="auth-links">
          <button
            class="auth-link-button"
            type="button"
            @click="goForgotPassword"
          >
            忘记密码？
          </button>
        </div>

        <AuthNotice v-if="auth.error" tone="error">{{ auth.error }}</AuthNotice>
        <p v-if="mode === 'register'" class="auth-hint">
          提示：注册成功后将自动登录；管理员权限需由现有管理员或命令行创建。
        </p>
      </form>

      <form v-else class="auth-form" @submit.prevent="submitEmailLogin">
        <div class="auth-field">
          <label class="auth-label" for="auth-email-login">邮箱</label>
          <div class="auth-input-wrap">
            <IconMail :size="16" class="auth-input-icon" />
            <input
              id="auth-email-login"
              v-model.trim="emailLogin"
              class="input auth-input"
              type="email"
              autocomplete="email"
              placeholder="user@example.com"
              :disabled="emailLoading"
            />
          </div>
        </div>

        <div class="auth-field">
          <label class="auth-label" for="auth-code">验证码</label>
          <div class="auth-code-row">
            <div class="auth-input-wrap">
              <IconKey :size="16" class="auth-input-icon" />
              <input
                id="auth-code"
                v-model.trim="emailCode"
                class="input auth-input"
                type="text"
                inputmode="numeric"
                autocomplete="one-time-code"
                placeholder="6 位验证码"
                :disabled="emailLoading"
              />
            </div>
            <button
              class="vf-ghost-button auth-code-send"
              type="button"
              :disabled="emailLoading"
              @click="sendEmailCode"
            >
              发送验证码
            </button>
          </div>
        </div>

        <button
          class="vf-ghost-button is-primary auth-submit"
          :class="{ 'is-loading': emailLoading }"
          :disabled="emailLoading"
          type="submit"
        >
          验证码登录
        </button>
      </form>
    </template>
  </AuthShell>
</template>

<script setup lang="ts">
import { computed, onMounted, ref, watch } from "vue";
import { useRouter, useRoute } from "vue-router";
import { IconKey, IconLock, IconMail, IconUser } from "@tabler/icons-vue";
import AuthShell from "../components/auth/AuthShell.vue";
import AuthNotice from "../components/auth/AuthNotice.vue";
import { useAuthStore } from "../stores/auth.store";
import { useAppStore } from "../stores/app.store";
import { authService } from "../services/auth.service";

const auth = useAuthStore();
const app = useAppStore();
const router = useRouter();
const route = useRoute();

type AuthMode = "login" | "register" | "email";

const mode = ref<AuthMode>("login");

/** 模式切换项：注册关闭时不显示注册。 */
const modeOptions = computed(() => {
  const options: { value: AuthMode; label: string }[] = [
    { value: "login", label: "登录" },
    { value: "email", label: "邮箱验证码" },
  ];
  if (auth.allowRegister) {
    options.push({ value: "register", label: "注册" });
  }
  return options;
});
const username = ref("");
const password = ref("");
const email = ref("");

const emailLogin = ref("");
const emailCode = ref("");
const emailLoading = ref(false);

const reasonText = computed(() => {
  const reason =
    typeof route.query.reason === "string" ? route.query.reason : "";
  if (reason === "expired") return "登录已过期，请重新登录。";
  return "";
});

watch(
  () => auth.allowRegister,
  (allow) => {
    if (!allow && mode.value === "register") mode.value = "login";
  },
);

function goHome() {
  router.push({ path: "/" });
}

function goForgotPassword() {
  router.push({ name: "forgot-password" });
}

async function submit() {
  if (!username.value || !password.value) {
    app.error("请输入用户名和密码");
    return;
  }

  try {
    if (mode.value === "login") {
      await auth.login(username.value, password.value);
      app.success("登录成功");
    } else {
      await auth.register(
        username.value,
        password.value,
        email.value || undefined,
      );
      app.success("注册成功");
    }

    // 等待 cookie 在浏览器中完全同步（移动端需要）
    await new Promise((r) => setTimeout(r, 100));
    await auth.fetchMe();

    const redirect =
      typeof route.query.redirect === "string" ? route.query.redirect : "/";
    router.replace(redirect);
  } catch (e) {
    app.error(e instanceof Error ? e.message : "操作失败");
  }
}

async function sendEmailCode() {
  if (!emailLogin.value) {
    app.error("请输入邮箱");
    return;
  }

  emailLoading.value = true;
  try {
    const res = await authService.requestEmailLoginCode(emailLogin.value);
    if (!res.success) throw new Error(res.error || "发送失败");
    app.success("如果邮箱已绑定账号，将收到验证码邮件");
  } catch (e) {
    app.error(e instanceof Error ? e.message : "发送失败");
  } finally {
    emailLoading.value = false;
  }
}

async function submitEmailLogin() {
  if (!emailLogin.value || !emailCode.value) {
    app.error("请输入邮箱和验证码");
    return;
  }

  emailLoading.value = true;
  try {
    const res = await authService.verifyEmailLoginCode(
      emailLogin.value,
      emailCode.value,
    );
    if (!res.success) throw new Error(res.error || "登录失败");

    // 标记登录成功时间，用于免疫期
    if (
      typeof window !== "undefined" &&
      (window as any).__vfiles_setLoginSuccess
    ) {
      (window as any).__vfiles_setLoginSuccess();
    }
    app.success("登录成功");

    // 等待 cookie 在浏览器中完全同步（移动端需要）
    await new Promise((r) => setTimeout(r, 100));
    await auth.fetchMe();

    const redirect =
      typeof route.query.redirect === "string" ? route.query.redirect : "/";
    router.replace(redirect);
  } catch (e) {
    app.error(e instanceof Error ? e.message : "登录失败");
  } finally {
    emailLoading.value = false;
  }
}

onMounted(async () => {
  if (!auth.initialized) {
    await auth.fetchMe();
  }

  if (auth.enabled && !auth.allowRegister && mode.value === "register") {
    mode.value = "login";
  }
});
</script>

<style scoped>
.auth-modes {
  display: flex;
  gap: 0.25rem;
  padding: 0.2rem;
  margin-bottom: 1rem;
  border: 1px solid var(--vf-border-weak);
  border-radius: var(--vf-radius);
  background: var(--vf-surface-sunken);
}

.auth-mode {
  flex: 1 1 0;
  min-width: 0;
  padding: 0.35rem 0.4rem;
  border: none;
  border-radius: var(--vf-radius-sm);
  background: transparent;
  color: var(--vf-text-muted);
  font-size: 0.82rem;
  cursor: pointer;
}

.auth-mode:hover {
  color: var(--vf-text-strong);
}

.auth-mode.is-active {
  background: var(--vf-surface);
  color: var(--vf-accent-strong);
  font-weight: 600;
  box-shadow: var(--vf-shadow-card);
}

.auth-form {
  display: flex;
  flex-direction: column;
  gap: 0.85rem;
}

.auth-field {
  display: flex;
  flex-direction: column;
  gap: 0.3rem;
}

.auth-label {
  color: var(--vf-text-muted);
  font-size: 0.78rem;
  font-weight: 600;
}

.auth-input-wrap {
  position: relative;
  display: flex;
  align-items: center;
  flex: 1 1 auto;
  min-width: 0;
}

.auth-input-icon {
  position: absolute;
  /* Bulma 的 .input 也是定位元素，不抬高层级时图标会被输入框底色盖住 */
  z-index: 1;
  left: 0.55rem;
  color: var(--vf-text-subtle);
  pointer-events: none;
}

.auth-input {
  padding-left: 2rem;
  font-size: 0.86rem;
}

.auth-code-row {
  display: flex;
  align-items: center;
  gap: 0.4rem;
}

.auth-code-send {
  flex: 0 0 auto;
  min-height: 2.1rem;
  font-size: 0.8rem;
}

.auth-submit {
  justify-content: center;
  width: 100%;
  min-height: 2.35rem;
  font-size: 0.9rem;
}

.auth-links {
  display: flex;
  justify-content: flex-end;
}

.auth-link-button {
  padding: 0;
  border: none;
  background: none;
  color: var(--vf-accent);
  font-size: 0.8rem;
  cursor: pointer;
}

.auth-link-button:hover,
.auth-link-button:focus-visible {
  text-decoration: underline;
}

.auth-hint {
  margin: 0;
  color: var(--vf-text-subtle);
  font-size: 0.78rem;
  line-height: 1.5;
}
</style>
