<template>
  <section class="section">
    <div class="container" style="max-width: 420px">
      <div class="box">
        <h1 class="title is-4">找回密码</h1>

        <p class="help mb-4">
          请输入绑定的邮箱。若该邮箱已绑定账号，系统将发送重置链接。
        </p>

        <form @submit.prevent="submit">
          <div class="field">
            <label class="label">邮箱</label>
            <div class="control">
              <input
                v-model.trim="email"
                class="input"
                type="email"
                autocomplete="email"
                placeholder="user@example.com"
                :disabled="loading"
              />
            </div>
          </div>

          <div class="field">
            <div class="control">
              <button
                class="button is-link is-fullwidth"
                :class="{ 'is-loading': loading }"
                :disabled="loading"
                type="submit"
              >
                发送重置邮件
              </button>
            </div>
          </div>

          <div class="buttons is-right">
            <button class="button is-light" type="button" :disabled="loading" @click="goLogin">
              返回登录
            </button>
          </div>
        </form>
      </div>
    </div>
  </section>
</template>

<script setup lang="ts">
import { ref } from "vue";
import { useRouter } from "vue-router";
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
