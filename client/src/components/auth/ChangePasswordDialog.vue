<template>
  <Modal
    :show="show"
    title="修改密码"
    :mobile-compact="true"
    @close="requestClose"
  >
    <form class="change-password-form" @submit.prevent="submit">
      <div class="field">
        <label class="label" for="change-password-current">当前密码</label>
        <div class="control">
          <input
            id="change-password-current"
            ref="currentInput"
            v-model="currentPassword"
            class="input"
            type="password"
            autocomplete="current-password"
            :disabled="loading"
          />
        </div>
      </div>

      <div class="field">
        <label class="label" for="change-password-new">新密码</label>
        <div class="control">
          <input
            id="change-password-new"
            v-model="newPassword"
            class="input"
            type="password"
            autocomplete="new-password"
            :disabled="loading"
          />
        </div>
        <p class="help">至少 8 个字符，且不能与当前密码相同。</p>
      </div>

      <div class="field">
        <label class="label" for="change-password-confirm">确认新密码</label>
        <div class="control">
          <input
            id="change-password-confirm"
            v-model="confirmPassword"
            class="input"
            type="password"
            autocomplete="new-password"
            :disabled="loading"
          />
        </div>
      </div>

      <p v-if="error" class="help is-danger" role="alert">{{ error }}</p>
      <p v-else class="help">
        修改成功后该账号的其它登录会话会失效，需要重新登录。
      </p>

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
          修改密码
        </button>
      </div>
    </form>
  </Modal>
</template>

<script setup lang="ts">
import { nextTick, ref, watch } from "vue";
import { useRouter } from "vue-router";
import Modal from "../common/Modal.vue";
import { authService } from "../../services/auth.service";
import { useAppStore } from "../../stores/app.store";
import { useAuthStore } from "../../stores/auth.store";

const props = defineProps<{ show: boolean }>();
const emit = defineEmits<{ (e: "close"): void }>();

const router = useRouter();
const app = useAppStore();
const auth = useAuthStore();

const currentPassword = ref("");
const newPassword = ref("");
const confirmPassword = ref("");
const error = ref<string | null>(null);
const loading = ref(false);
const currentInput = ref<HTMLInputElement | null>(null);

function reset() {
  currentPassword.value = "";
  newPassword.value = "";
  confirmPassword.value = "";
  error.value = null;
  loading.value = false;
}

watch(
  () => props.show,
  (visible) => {
    if (!visible) {
      reset();
      return;
    }
    error.value = null;
    void nextTick(() => currentInput.value?.focus());
  },
);

function requestClose() {
  if (loading.value) return;
  emit("close");
}

async function submit() {
  error.value = null;
  if (!currentPassword.value) {
    error.value = "请输入当前密码";
    return;
  }
  if (newPassword.value.length < 8) {
    error.value = "新密码至少 8 个字符";
    return;
  }
  if (newPassword.value === currentPassword.value) {
    error.value = "新密码不能与当前密码相同";
    return;
  }
  if (newPassword.value !== confirmPassword.value) {
    error.value = "两次输入的新密码不一致";
    return;
  }

  loading.value = true;
  try {
    await authService.changePassword(currentPassword.value, newPassword.value);
    app.success("密码已修改，请重新登录");
    emit("close");
    reset();
    // 服务端已撤销全部会话：清掉本地状态并回登录页。
    try {
      await auth.logout();
    } catch {
      // 会话已失效时 logout 可能返回 401；本地清理在 store 的 finally 中完成。
    }
    await router.push("/login");
  } catch (e) {
    error.value = e instanceof Error ? e.message : "修改密码失败";
  } finally {
    loading.value = false;
  }
}
</script>

<style scoped>
.change-password-form .help {
  margin-top: var(--vf-space-1);
}
</style>
