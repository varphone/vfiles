<template>
  <AuthShell
    title="找回密码"
    description="请输入绑定的邮箱。若该邮箱已绑定账号，系统将发送重置链接。"
  >
    <form class="auth-form" @submit.prevent="submit">
      <div class="auth-field">
        <label class="auth-label" for="forgot-email">邮箱</label>
        <div class="auth-input-wrap">
          <IconMail :size="16" class="auth-input-icon" />
          <input
            id="forgot-email"
            v-model.trim="email"
            class="input auth-input"
            type="email"
            autocomplete="email"
            placeholder="user@example.com"
            :disabled="loading"
          />
        </div>
      </div>

      <button
        class="vf-ghost-button is-primary auth-submit"
        :class="{ 'is-loading': loading }"
        :disabled="loading"
        type="submit"
      >
        发送重置邮件
      </button>

      <div class="auth-links">
        <button class="auth-link-button" type="button" @click="goLogin">
          返回登录
        </button>
      </div>
    </form>
  </AuthShell>
</template>

<script setup lang="ts">
import { ref } from "vue";
import { useRouter } from "vue-router";
import { IconMail } from "@tabler/icons-vue";
import AuthShell from "../components/auth/AuthShell.vue";
import { authService } from "../services/auth.service";
import { useAppStore } from "../stores/app.store";

const router = useRouter();
const app = useAppStore();

const email = ref("");
const loading = ref(false);

function goLogin() {
  router.push({ name: "login" });
}

async function submit() {
  if (!email.value) {
    app.error("请输入邮箱");
    return;
  }

  loading.value = true;
  try {
    const res = await authService.requestPasswordReset(email.value);
    if (!res.success) throw new Error(res.error || "发送失败");
    app.success("如果邮箱已绑定账号，将收到重置邮件");
    goLogin();
  } catch (e) {
    app.error(e instanceof Error ? e.message : "发送失败");
  } finally {
    loading.value = false;
  }
}
</script>

<style scoped>
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
</style>
