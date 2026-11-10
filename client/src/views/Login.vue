<template>
  <AuthShell :title="cardTitle">
    <AuthNotice v-if="auth.initialized && auth.enabled === false" tone="info">
      当前服务未启用认证（ENABLE_AUTH=false）。
      <button class="auth-link-button" type="button" @click="goHome">
        返回首页
      </button>
    </AuthNotice>

    <template v-else>
      <AuthNotice v-if="reasonText" tone="warning">{{ reasonText }}</AuthNotice>

      <!-- 模式切换：登录 / 邮箱验证码 / 注册。
           按 Nextcloud 的「只有表单」观感做成低调的顶部小标签（无边框、选中项下划线），
           功能与 role=tab 语义保持不变。 -->
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
          <label class="auth-label" for="auth-username">账号或邮箱</label>
          <div class="auth-input-wrap">
            <input
              id="auth-username"
              aria-required="true"
              v-model.trim="username"
              class="input auth-input"
              type="text"
              autocomplete="username"
              placeholder="3-32 位，字母数字-_"
              :aria-invalid="fieldErrors.username ? 'true' : undefined"
              :aria-describedby="
                fieldErrors.username ? 'auth-username-error' : undefined
              "
              :disabled="auth.loading"
            />
          </div>
          <!-- 行内错误 + 字段级 aria 链（r86 样板族推广 ✓） -->
          <p
            v-if="fieldErrors.username"
            id="auth-username-error"
            class="auth-field-error"
            role="alert"
          >
            {{ fieldErrors.username }}
          </p>
        </div>

        <div v-if="mode === 'register'" class="auth-field">
          <label class="auth-label" for="auth-email">邮箱（可选）</label>
          <div class="auth-input-wrap">
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
            <input
              id="auth-password"
              aria-required="true"
              v-model="password"
              class="input auth-input auth-input-password"
              :type="showPassword ? 'text' : 'password'"
              :autocomplete="
                mode === 'login' ? 'current-password' : 'new-password'
              "
              placeholder="至少 6 位"
              :aria-invalid="fieldErrors.password ? 'true' : undefined"
              :aria-describedby="
                fieldErrors.password ? 'auth-password-error' : undefined
              "
              :disabled="auth.loading"
            />
            <button
              class="auth-password-toggle"
              type="button"
              aria-controls="auth-password"
              :aria-label="showPassword ? '隐藏密码' : '显示密码'"
              :aria-pressed="showPassword ? 'true' : 'false'"
              :title="showPassword ? '隐藏密码' : '显示密码'"
              :disabled="auth.loading"
              @click="showPassword = !showPassword"
            >
              <component
                :is="showPassword ? IconEyeOff : IconEye"
                :size="18"
                aria-hidden="true"
              />
            </button>
          </div>
          <p
            v-if="fieldErrors.password"
            id="auth-password-error"
            class="auth-field-error"
            role="alert"
          >
            {{ fieldErrors.password }}
          </p>
        </div>

        <!-- 记住我：只记住账号名（便于下次填充），会话时效仍由服务端 Cookie 决定。 -->
        <label v-if="mode === 'login'" class="auth-remember">
          <input
            v-model="remember"
            class="auth-checkbox"
            type="checkbox"
            :disabled="auth.loading"
          />
          <span>记住我</span>
        </label>

        <button
          class="auth-submit auth-primary"
          data-variant="primary"
          :disabled="auth.loading"
          type="submit"
        >
          <span>{{ mode === "login" ? "登录" : "注册" }}</span>
          <IconArrowRight
            :size="18"
            aria-hidden="true"
            class="auth-submit-icon"
          />
        </button>

        <AuthNotice v-if="auth.error" tone="error">{{ auth.error }}</AuthNotice>
        <p v-if="mode === 'register'" class="auth-hint">
          提示：注册成功后将自动登录；管理员权限需由现有管理员或命令行创建。
        </p>

        <!--
          设备登录 = 访问令牌页（/settings/tokens）。未登录时全局守卫会把它重定向回
          登录页，若在同一标签内 router-link 跳转，只会刷新登录地址并丢掉已填字段；
          因此用 target="_blank" 打开：已有会话时直达令牌页，没有会话时在新标签完成
          登录后落到令牌页，不打断当前登录流程。
        -->
        <div v-if="mode === 'login'" class="auth-links">
          <a
            class="auth-link-button"
            href="/settings/tokens"
            target="_blank"
            rel="noopener noreferrer"
          >
            使用设备登录
          </a>
          <button
            class="auth-link-button"
            type="button"
            @click="goForgotPassword"
          >
            忘记密码？
          </button>
        </div>
      </form>

      <form v-else class="auth-form" @submit.prevent="submitEmailLogin">
        <div class="auth-field">
          <label class="auth-label" for="auth-email-login">邮箱</label>
          <div class="auth-input-wrap">
            <input
              id="auth-email-login"
              aria-required="true"
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
              class="auth-code-send"
              type="button"
              :disabled="emailLoading"
              @click="sendEmailCode"
            >
              发送验证码
            </button>
          </div>
        </div>

        <button
          class="auth-submit auth-primary"
          data-variant="primary"
          :disabled="emailLoading"
          type="submit"
        >
          <span>验证码登录</span>
          <IconArrowRight
            :size="18"
            aria-hidden="true"
            class="auth-submit-icon"
          />
        </button>
      </form>
    </template>
  </AuthShell>
</template>

<script setup lang="ts">
import { computed, onMounted, ref, watch } from "vue";
import { useRouter, useRoute } from "vue-router";
import { IconArrowRight, IconEye, IconEyeOff } from "@tabler/icons-vue";
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

/** 卡片标题随模式变化：登录页固定为「登录 VFiles」。 */
const cardTitle = computed(() => {
  if (mode.value === "email") return "邮箱验证码登录";
  if (mode.value === "register") return "注册 VFiles";
  return "登录 VFiles";
});

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

/** 密码显隐：只影响输入框 type，不参与提交值。 */
const showPassword = ref(false);

const emailLogin = ref("");
const emailCode = ref("");
const emailLoading = ref(false);

/**
 * 记住我：仅将账号名写入 localStorage 以便下次打开时预填；
 * 会话有效期依旧由服务端下发的 Cookie 决定（本页不改登录协议）。
 */
const REMEMBER_KEY = "vfiles.login.remember-username";
const remember = ref(false);

function readRememberedUsername(): string {
  try {
    return localStorage.getItem(REMEMBER_KEY) ?? "";
  } catch {
    return "";
  }
}

function rememberUsername(): void {
  try {
    if (remember.value) {
      localStorage.setItem(REMEMBER_KEY, username.value);
    } else {
      localStorage.removeItem(REMEMBER_KEY);
    }
  } catch {
    // 隐私模式禁用本地存储：忽略即可，不影响登录流程。
  }
}

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

const fieldErrors = ref<{ username?: string; password?: string }>({});

async function submit() {
  // 行内校验（r86 样板族推广）：必填 + 注册密码 ≥6（服务端同规则 ✓ 前端先行）
  fieldErrors.value = {};
  if (!username.value) fieldErrors.value.username = "请输入用户名";
  if (!password.value) {
    fieldErrors.value.password = "请输入密码";
  } else if (mode.value === "register" && password.value.length < 6) {
    fieldErrors.value.password = "密码至少 6 位";
  }
  if (Object.keys(fieldErrors.value).length > 0) return;

  try {
    if (mode.value === "login") {
      await auth.login(username.value, password.value);
      rememberUsername();
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
  const remembered = readRememberedUsername();
  if (remembered && !username.value) {
    username.value = remembered;
    remember.value = true;
  }

  if (!auth.initialized) {
    await auth.fetchMe();
  }

  if (auth.enabled && !auth.allowRegister && mode.value === "register") {
    mode.value = "login";
  }
});
</script>

<style scoped>
/* 顶部模式标签：无边框、无底色，选中项只用 2px 下划线区分（保持表单观感）。 */
.auth-modes {
  display: flex;
  justify-content: center;
  gap: 0.25rem;
  margin: -0.25rem 0 1rem;
}

.auth-mode {
  padding: 0.25rem 0.5rem 0.375rem;
  border: none;
  border-bottom: 2px solid transparent;
  border-radius: 0;
  background: transparent;
  color: var(--auth-card-muted);
  font-size: 0.75rem;
  font-weight: 600;
  cursor: pointer;
}

.auth-mode:hover {
  color: var(--auth-card-ink);
}

.auth-mode.is-active {
  border-bottom-color: var(--auth-card-ink);
  color: var(--auth-card-ink);
}

.auth-mode:focus-visible {
  outline: 2px solid var(--auth-primary-bg);
  outline-offset: 1px;
}

/* 记住我：13px、深海军蓝勾选框。 */
.auth-remember {
  display: inline-flex;
  align-items: center;
  align-self: flex-start;
  gap: 0.5rem;
  color: var(--auth-card-ink);
  font-size: 13px;
  line-height: 1.4;
  cursor: pointer;
  user-select: none;
}

.auth-checkbox {
  width: 16px;
  height: 16px;
  margin: 0;
  /* 原生复选框着色：选中即为主色强调色的勾，无额外 DOM。 */
  accent-color: var(--auth-field-border);
  cursor: pointer;
}

.auth-checkbox:focus-visible {
  outline: 2px solid var(--auth-primary-bg);
  outline-offset: 2px;
}

.auth-checkbox:disabled {
  opacity: 0.5;
  cursor: default;
}

/* 邮箱验证码：输入框 + 发送按钮同一行。 */
.auth-code-row {
  display: flex;
  align-items: center;
  gap: 0.5rem;
}

.auth-code-send {
  flex: 0 0 auto;
  box-sizing: border-box;
  height: 44px;
  padding: 0 0.75rem;
  border: 2px solid var(--auth-field-border);
  border-radius: 3px;
  background: transparent;
  color: var(--auth-field-border);
  font-size: 0.8125rem;
  font-weight: 600;
  white-space: nowrap;
  cursor: pointer;
}

.auth-code-send:hover:not(:disabled) {
  background: var(--auth-field-hover);
}

.auth-code-send:focus-visible {
  outline: 2px solid var(--auth-primary-bg);
  outline-offset: 1px;
}

.auth-code-send:disabled {
  opacity: 0.5;
  cursor: default;
}
</style>
