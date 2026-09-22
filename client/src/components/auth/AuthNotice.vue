<template>
  <p
    class="auth-notice"
    :class="`is-${tone}`"
    :role="tone === 'error' ? 'alert' : 'status'"
  >
    <component
      :is="icon"
      :size="16"
      class="auth-notice-icon"
      aria-hidden="true"
    />
    <span class="auth-notice-text"><slot /></span>
  </p>
</template>

<script setup lang="ts">
import { computed } from "vue";
import {
  IconAlertCircle,
  IconAlertTriangle,
  IconCheck,
  IconInfoCircle,
} from "@tabler/icons-vue";

/**
 * 认证页提示条：信息 / 警告 / 错误 / 成功四种语气。
 *
 * 之前登录页用 Bulma 的 notification，找回页用 help 文本，风格不统一。
 */
const props = withDefaults(
  defineProps<{ tone?: "info" | "warning" | "error" | "success" }>(),
  { tone: "info" },
);

const icon = computed(() => {
  switch (props.tone) {
    case "error":
      return IconAlertCircle;
    case "warning":
      return IconAlertTriangle;
    case "success":
      return IconCheck;
    default:
      return IconInfoCircle;
  }
});
</script>

<style scoped>
.auth-notice {
  display: flex;
  align-items: flex-start;
  gap: 0.4rem;
  margin: 0 0 0.9rem;
  padding: 0.5rem 0.6rem;
  border: 1px solid transparent;
  border-radius: var(--vf-radius-sm);
  font-size: 0.8rem;
  line-height: 1.5;
}

.auth-notice-icon {
  flex: 0 0 auto;
  margin-top: 0.1rem;
}

.auth-notice-text {
  min-width: 0;
}

.auth-notice.is-info {
  border-color: var(--vf-border-weak);
  background: var(--vf-surface-sunken);
  color: var(--vf-text-muted);
}

.auth-notice.is-warning {
  border-color: var(--vf-warning-line);
  background: var(--vf-warning-soft);
  color: var(--vf-text);
}

.auth-notice.is-warning .auth-notice-icon {
  color: var(--vf-warning-text);
}

.auth-notice.is-error {
  border-color: var(--vf-danger-soft);
  background: var(--vf-danger-soft);
  color: var(--vf-danger-text);
}

.auth-notice.is-success {
  border-color: var(--vf-success-soft);
  background: var(--vf-success-soft);
  color: var(--vf-success-text);
}
</style>
