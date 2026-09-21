<template>
  <div
    class="notifications"
    role="status"
    aria-live="polite"
    aria-relevant="additions"
  >
    <transition-group name="notification">
      <div
        v-for="notification in notifications"
        :key="notification.id"
        :class="['notification', `is-${notification.type}`, 'toast']"
      >
        <span class="toast-icon" aria-hidden="true">
          <IconCircleCheck v-if="notification.type === 'success'" :size="18" />
          <IconAlertCircle
            v-else-if="notification.type === 'error'"
            :size="18"
          />
          <IconAlertTriangle
            v-else-if="notification.type === 'warning'"
            :size="18"
          />
          <IconInfoCircle v-else :size="18" />
        </span>

        <p class="toast-message">{{ notification.message }}</p>

        <button
          class="delete is-small toast-close"
          :aria-label="`关闭通知：${notification.message}`"
          @click="remove(notification.id)"
        ></button>
      </div>
    </transition-group>
  </div>
</template>

<script setup lang="ts">
import { storeToRefs } from "pinia";
import {
  IconAlertCircle,
  IconAlertTriangle,
  IconCircleCheck,
  IconInfoCircle,
} from "@tabler/icons-vue";
import { useAppStore } from "../../stores/app.store";

const appStore = useAppStore();
const { notifications } = storeToRefs(appStore);

function remove(id: number) {
  appStore.removeNotification(id);
}
</script>

<style scoped>
.notifications {
  position: fixed;
  top: calc(var(--bulma-navbar-height, 3.25rem) + 0.75rem);
  right: 1rem;
  z-index: 10000;
  display: flex;
  flex-direction: column;
  gap: 0.5rem;
  width: min(24rem, calc(100vw - 2rem));
  pointer-events: none;
}

/* 移动端提示条放到底部：顶部会盖住面包屑与搜索行（主流移动端同样如此） */
@media screen and (max-width: 1023px) {
  .notifications {
    top: auto;
    bottom: calc(4.9rem + env(safe-area-inset-bottom) + var(--vv-bottom, 0px));
    left: 0.75rem;
    right: 0.75rem;
    width: auto;
  }
}

/* 卡片式提示：图标 + 文案 + 关闭，颜色仍按语义区分 */
.toast {
  display: flex;
  align-items: flex-start;
  gap: 0.55rem;
  margin-bottom: 0;
  padding: 0.7rem 0.85rem;
  border: 1px solid var(--vf-border-weak);
  border-left-width: 3px;
  border-radius: var(--vf-radius);
  background: var(--vf-surface-raised);
  color: var(--vf-text);
  box-shadow: var(--vf-shadow-menu);
  pointer-events: auto;
}

.toast.is-success {
  border-left-color: var(--vf-success-text);
}

.toast.is-error {
  border-left-color: var(--vf-danger-text);
}

.toast.is-warning {
  border-left-color: var(--vf-warning-text);
}

.toast.is-info {
  border-left-color: var(--vf-accent);
}

.toast-icon {
  flex: 0 0 auto;
  margin-top: 0.1rem;
}

.toast.is-success .toast-icon {
  color: var(--vf-success-text);
}

.toast.is-error .toast-icon {
  color: var(--vf-danger-text);
}

.toast.is-warning .toast-icon {
  color: var(--vf-warning-text);
}

.toast.is-info .toast-icon {
  color: var(--vf-accent);
}

.toast-message {
  flex: 1 1 auto;
  min-width: 0;
  font-size: 0.84rem;
  line-height: 1.4;
  word-break: break-word;
}

.toast-close {
  flex: 0 0 auto;
  margin-top: 0.15rem;
}

.notification-enter-active,
.notification-leave-active {
  transition:
    opacity 0.25s ease,
    transform 0.25s ease;
}

.notification-enter-from {
  opacity: 0;
  transform: translateX(1rem);
}

.notification-leave-to {
  opacity: 0;
  transform: translateX(1rem);
}

@media (prefers-reduced-motion: reduce) {
  .notification-enter-active,
  .notification-leave-active {
    transition: none;
  }
}
</style>
