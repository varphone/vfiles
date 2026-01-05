<template>
  <section class="section">
    <div class="container" style="max-width: 420px">
      <div class="box">
        <h1 class="title is-4">VFiles 登录</h1>

        <div v-if="auth.initialized && auth.enabled === false" class="notification is-info is-light">
          当前服务未启用认证（ENABLE_AUTH=false）。
          <div class="mt-3">
            <button class="button is-link" @click="goHome">返回首页</button>
          </div>
        </div>

        <template v-else>
          <div
            v-if="reasonText"
            class="notification is-warning is-light"
          >
            {{ reasonText }}
          </div>

          <div class="tabs is-toggle is-fullwidth">
            <ul>
              <li :class="{ 'is-active': mode === 'login' }">
                <a href="#" @click.prevent="mode = 'login'">登录</a>
              </li>
              <li :class="{ 'is-active': mode === 'email' }">
                <a href="#" @click.prevent="mode = 'email'">邮箱验证码</a>
              </li>
              <li
                v-if="auth.allowRegister"
                :class="{ 'is-active': mode === 'register' }"
              >
                <a href="#" @click.prevent="mode = 'register'">注册</a>
              </li>
            </ul>
          </div>

          <div
            v-if="auth.enabled && !auth.allowRegister"
            class="notification is-info is-light"
          >
            管理员已关闭注册（AUTH_ALLOW_REGISTER=false）。
          </div>

          <form v-if="mode !== 'email'" @submit.prevent="submit">
            <div class="field">
              <label class="label">用户名</label>
              <div class="control">
                <input
                  v-model.trim="username"
                  class="input"
                  type="text"
                  autocomplete="username"
                  placeholder="3-32 位，字母数字-_"
                  :disabled="auth.loading"
                />
              </div>
            </div>

            <div v-if="mode === 'register'" class="field">
              <label class="label">邮箱（可选）</label>
              <div class="control">
                <input
                  v-model.trim="email"
                  class="input"
                  type="email"
                  autocomplete="email"
                  placeholder="用于找回密码/验证码登录"
                  :disabled="auth.loading"
                />
              </div>
            </div>

            <div class="field">
              <label class="label">密码</label>
              <div class="control">
                <input
                  v-model="password"
                  class="input"
                  type="password"
                  :autocomplete="mode === 'login' ? 'current-password' : 'new-password'"
                  placeholder="至少 6 位"
                  :disabled="auth.loading"
                />
              </div>
            </div>

            <div v-if="mode === 'login'" class="field">
              <a href="#" class="is-size-7" @click.prevent="goForgotPassword">忘记密码？</a>
            </div>

            <div class="field">
              <div class="control">
                <button
                  class="button is-link is-fullwidth"
                  :class="{ 'is-loading': auth.loading }"
                  :disabled="auth.loading"
                  type="submit"
                >
                  {{ mode === 'login' ? '登录' : '注册' }}
                </button>
              </div>
            </div>

            <p v-if="auth.error" class="help is-danger">{{ auth.error }}</p>
            <p class="help">
              提示：首次注册用户会自动成为 <code>admin</code>。
            </p>
          </form>

          <form v-else @submit.prevent="submitEmailLogin">
            <div class="field">
              <label class="label">邮箱</label>
              <div class="control">
                <input
                  v-model.trim="emailLogin"
                  class="input"
                  type="email"
                  autocomplete="email"
                  placeholder="user@example.com"
                  :disabled="emailLoading"
                />
              </div>
            </div>

            <div class="field has-addons">
              <div class="control is-expanded">
                <input
                  v-model.trim="emailCode"
                  class="input"
                  type="text"
                  inputmode="numeric"
                  autocomplete="one-time-code"
                  placeholder="6 位验证码"
                  :disabled="emailLoading"
                />
              </div>
              <div class="control">
                <button
                  class="button is-light"
                  type="button"
                  :disabled="emailLoading"
                  @click="sendEmailCode"
                >
                  发送验证码
                </button>
              </div>
            </div>

            <div class="field">
              <div class="control">
                <button
                  class="button is-link is-fullwidth"
                  :class="{ 'is-loading': emailLoading }"
                  :disabled="emailLoading"
                  type="submit"
                >
                  验证码登录
                </button>
              </div>
            </div>
          </form>
        </template>
      </div>
    </div>
  </section>
</template>

<script setup lang="ts">
import { computed, onMounted, ref, watch } from "vue";
import { useRouter, useRoute } from "vue-router";
import { useAuthStore } from "../stores/auth.store";
import { useAppStore } from "../stores/app.store";
import { authService } from "../services/auth.service";

const auth = useAuthStore();
const app = useAppStore();
const router = useRouter();
const route = useRoute();

const mode = ref<"login" | "register" | "email">("login");
const username = ref("");
const password = ref("");
const email = ref("");

const emailLogin = ref("");
const emailCode = ref("");
const emailLoading = ref(false);

const reasonText = computed(() => {
  const reason = typeof route.query.reason === "string" ? route.query.reason : "";
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
      await auth.register(username.value, password.value, email.value || undefined);
      app.success("注册成功");
    }

    const redirect = typeof route.query.redirect === "string" ? route.query.redirect : "/";
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

    await auth.fetchMe();
    app.success("登录成功");

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
