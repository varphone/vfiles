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
          <div class="tabs is-toggle is-fullwidth">
            <ul>
              <li :class="{ 'is-active': mode === 'login' }">
                <a href="#" @click.prevent="mode = 'login'">登录</a>
              </li>
              <li :class="{ 'is-active': mode === 'register' }">
                <a href="#" @click.prevent="mode = 'register'">注册</a>
              </li>
            </ul>
          </div>

          <form @submit.prevent="submit">
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

            <div class="field">
              <label class="label">密码</label>
              <div class="control">
                <input
                  v-model="password"
                  class="input"
                  type="password"
                  autocomplete="current-password"
                  placeholder="至少 6 位"
                  :disabled="auth.loading"
                />
              </div>
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
        </template>
      </div>
    </div>
  </section>
</template>

<script setup lang="ts">
import { onMounted, ref } from "vue";
import { useRouter, useRoute } from "vue-router";
import { useAuthStore } from "../stores/auth.store";
import { useAppStore } from "../stores/app.store";

const auth = useAuthStore();
const app = useAppStore();
const router = useRouter();
const route = useRoute();

const mode = ref<"login" | "register">("login");
const username = ref("");
const password = ref("");

function goHome() {
  router.push({ path: "/" });
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
      await auth.register(username.value, password.value);
      app.success("注册成功");
    }

    const redirect = typeof route.query.redirect === "string" ? route.query.redirect : "/";
    router.replace(redirect);
  } catch (e) {
    app.error(e instanceof Error ? e.message : "操作失败");
  }
}

onMounted(async () => {
  if (!auth.initialized) {
    await auth.fetchMe();
  }
});
</script>
