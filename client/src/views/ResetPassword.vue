<template>
  <AuthShell
    title="重置密码"
    description="粘贴邮件中的重置 token，并设置新的登录密码。"
  >
    <form class="auth-form" @submit.prevent="submit">
      <div class="auth-field">
        <label class="auth-label" for="reset-token">重置 Token</label>
        <div class="auth-input-wrap">
          <IconKey :size="16" class="auth-input-icon" />
          <input
            id="reset-token"
            v-model.trim="token"
            class="input auth-input"
            type="text"
            placeholder="粘贴邮件中的 token"
            :disabled="loading"
          />
        </div>
      </div>

      <div class="auth-field">
        <label class="auth-label" for="reset-password">新密码</label>
        <div class="auth-input-wrap">
          <IconLock :size="16" class="auth-input-icon" />
          <input
            id="reset-password"
            v-model="newPassword"
            class="input auth-input"
            type="password"
            autocomplete="new-password"
            placeholder="至少 6 位"
            :disabled="loading"
          />
        </div>
      </div>

      <div class="auth-field">
        <label class="auth-label" for="reset-confirm">确认新密码</label>
        <div class="auth-input-wrap">
          <IconLock :size="16" class="auth-input-icon" />
          <input
            id="reset-confirm"
            v-model="confirmPassword"
            class="input auth-input"
            type="password"
            autocomplete="new-password"
            placeholder="再次输入"
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
        重置密码
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
import { onMounted, ref } from "vue";
import { useRouter, useRoute } from "vue-router";
import { IconKey, IconLock } from "@tabler/icons-vue";
import AuthShell from "../components/auth/AuthShell.vue";
import { authService } from "../services/auth.service";
import { useAppStore } from "../stores/app.store";

const router = useRouter();
const route = useRoute();
const app = useAppStore();

const token = ref("");
const newPassword = ref("");
const confirmPassword = ref("");
const loading = ref(false);

function goLogin() {
  router.push({ name: "login" });
}

async function submit() {
  if (!token.value) {
    app.error("请输入 token");
    return;
  }
  if (!newPassword.value) {
    app.error("请输入新密码");
    return;
  }
  if (newPassword.value !== confirmPassword.value) {
    app.error("两次输入的密码不一致");
    return;
  }

  loading.value = true;
  try {
    const res = await authService.confirmPasswordReset(
      token.value,
      newPassword.value,
    );
    if (!res.success) throw new Error(res.error || "重置失败");
    app.success("密码已重置，请重新登录");
    goLogin();
  } catch (e) {
    app.error(e instanceof Error ? e.message : "重置失败");
  } finally {
    loading.value = false;
  }
}

onMounted(() => {
  const q = typeof route.query.token === "string" ? route.query.token : "";
  if (q) token.value = q;
});
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
