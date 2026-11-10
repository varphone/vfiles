<template>
  <Modal :show="show" title="个人资料" :mobile-compact="true" @close="requestClose">
    <form class="profile-form" novalidate @submit.prevent="submit">
      <div class="field">
        <label class="label" for="profile-username">用户名</label>
        <div class="control">
          <input
            id="profile-username"
            class="input"
            type="text"
            :value="auth.user?.username || ''"
            disabled
          />
        </div>
      </div>

      <div class="field">
        <label class="label" for="profile-email">邮箱</label>
        <div class="control">
          <input
            id="profile-email"
            ref="emailInput"
            v-model="email"
            class="input"
            type="email"
            autocomplete="email"
            required
            :disabled="loading"
          />
        </div>
        <p class="help">用于登录与账号通知，修改后立即生效。</p>
      </div>

      <p v-if="error" class="help is-danger" role="alert">{{ error }}</p>

      <div class="buttons is-right mt-4">
        <button
          class="button"
          type="button"
          :disabled="loading"
          @click="requestClose"
        >
          取消
        </button>
        <button
          class="button is-primary"
          type="submit"
          :class="{ 'is-loading': loading }"
          :disabled="loading"
        >
          保存
        </button>
      </div>
    </form>
  </Modal>
</template>

<script setup lang="ts">
import { nextTick, ref, watch } from "vue";
import Modal from "../common/Modal.vue";
import { authService } from "../../services/auth.service";
import { useAppStore } from "../../stores/app.store";
import { useAuthStore } from "../../stores/auth.store";

const props = defineProps<{ show: boolean }>();
const emit = defineEmits<{ (e: "close"): void }>();

const app = useAppStore();
const auth = useAuthStore();

const email = ref("");
const error = ref<string | null>(null);
const loading = ref(false);
const emailInput = ref<HTMLInputElement | null>(null);

watch(
  () => props.show,
  (visible) => {
    if (!visible) return;
    email.value = auth.user?.email || "";
    error.value = null;
    loading.value = false;
    void nextTick(() => emailInput.value?.focus());
  },
);

function requestClose() {
  if (loading.value) return;
  emit("close");
}

async function submit() {
  error.value = null;
  const value = email.value.trim();
  if (!value) {
    error.value = "请输入邮箱";
    return;
  }
  if (value === (auth.user?.email || "")) {
    emit("close");
    return;
  }

  loading.value = true;
  try {
    await authService.updateProfile(value);
    // 会话快照里只有邮箱会变化，本地同步即可，避免额外一次 /auth/me。
    if (auth.user) auth.user.email = value;
    app.success("个人资料已更新");
    emit("close");
  } catch (e) {
    error.value = e instanceof Error ? e.message : "保存失败";
  } finally {
    loading.value = false;
  }
}
</script>

<style scoped>
.profile-form .help {
  margin-top: var(--vf-space-1);
}
</style>
