<template>
  <AuthShell
    title="找回密码"
    description="请输入绑定的邮箱。若该邮箱已绑定账号，系统将发送重置链接。"
  >
    <form class="auth-form" @submit.prevent="submit">
      <div class="auth-field">
        <label class="auth-label" for="forgot-email">邮箱</label>
        <div class="auth-input-wrap">
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
        class="auth-submit auth-primary"
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
