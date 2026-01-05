<template>
  <section class="section">
    <div class="container" style="max-width: 420px">
      <div class="box">
        <h1 class="title is-4">重置密码</h1>

        <form @submit.prevent="submit">
          <div class="field">
            <label class="label">重置 Token</label>
            <div class="control">
              <input
                v-model.trim="token"
                class="input"
                type="text"
                placeholder="粘贴邮件中的 token"
                :disabled="loading"
              />
            </div>
          </div>

          <div class="field">
            <label class="label">新密码</label>
            <div class="control">
              <input
                v-model="newPassword"
                class="input"
                type="password"
                autocomplete="new-password"
                placeholder="至少 6 位"
                :disabled="loading"
              />
            </div>
          </div>

          <div class="field">
            <label class="label">确认新密码</label>
            <div class="control">
              <input
                v-model="confirmPassword"
                class="input"
                type="password"
                autocomplete="new-password"
                placeholder="再次输入"
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
                重置密码
              </button>
            </div>
          </div>

          <div class="buttons is-right">
            <button
              class="button is-light"
              type="button"
              :disabled="loading"
              @click="goLogin"
            >
              返回登录
            </button>
          </div>
        </form>
      </div>
    </div>
  </section>
</template>

<script setup lang="ts">
import { onMounted, ref } from "vue";
import { useRouter, useRoute } from "vue-router";
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
