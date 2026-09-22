<template>
  <div
    ref="rootRef"
    class="dropdown is-right notification-center"
    :class="{ 'is-active': open }"
  >
    <div class="dropdown-trigger">
      <button
        class="vf-icon-button app-bar-bell"
        :class="{ 'is-active': open }"
        type="button"
        aria-haspopup="true"
        :aria-expanded="open ? 'true' : 'false'"
        :title="bellTitle"
        :aria-label="bellTitle"
        @click="toggle"
      >
        <IconBell :size="18" />
        <span
          v-if="app.unreadNotifications > 0"
          class="app-bar-bell-badge"
          aria-hidden="true"
        >
          {{ app.unreadNotifications > 9 ? "9+" : app.unreadNotifications }}
        </span>
      </button>
    </div>

    <div class="dropdown-menu notification-center-menu" role="menu">
      <div class="notification-center-panel">
        <header class="notification-center-head">
          <span class="notification-center-title">通知</span>
          <button
            v-if="app.notificationHistory.length > 0"
            class="notification-center-clear"
            type="button"
            @click="app.clearNotificationHistory()"
          >
            清空
          </button>
        </header>

        <div
          v-if="app.notificationHistory.length > 0"
          class="notification-center-filters"
          role="tablist"
          aria-label="按类型筛选通知"
        >
          <button
            v-for="filter in filters"
            :key="filter.value"
            class="notification-center-filter"
            :class="{ 'is-active': activeFilter === filter.value }"
            type="button"
            role="tab"
            :aria-selected="activeFilter === filter.value ? 'true' : 'false'"
            @click="activeFilter = filter.value"
          >
            {{ filter.label }}
          </button>
        </div>

        <p
          v-if="app.notificationHistory.length === 0"
          class="notification-center-empty"
        >
          暂无通知
        </p>

        <p
          v-else-if="visibleHistory.length === 0"
          class="notification-center-empty"
        >
          没有{{ activeFilterLabel }}通知
        </p>

        <ul v-else class="notification-center-list">
          <li
            v-for="item in visibleHistory"
            :key="item.id"
            class="notification-center-item"
            :class="`is-${item.type}`"
          >
            <span class="notification-center-icon" aria-hidden="true">
              <IconCircleCheck v-if="item.type === 'success'" :size="16" />
              <IconAlertCircle v-else-if="item.type === 'error'" :size="16" />
              <IconAlertTriangle
                v-else-if="item.type === 'warning'"
                :size="16"
              />
              <IconInfoCircle v-else :size="16" />
            </span>
            <div class="notification-center-body">
              <p class="notification-center-message">{{ item.message }}</p>
              <p class="notification-center-time">
                {{ formatRelativeDate(new Date(item.createdAt).toISOString()) }}
              </p>
            </div>
            <button
              class="notification-center-remove"
              type="button"
              :aria-label="`移除通知：${item.message}`"
              :title="`移除通知：${item.message}`"
              @click="app.removeNotificationFromHistory(item.id)"
            >
              <IconX :size="14" />
            </button>
          </li>
        </ul>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref } from "vue";
import {
  IconAlertCircle,
  IconAlertTriangle,
  IconBell,
  IconCircleCheck,
  IconInfoCircle,
  IconX,
} from "@tabler/icons-vue";
import { useAppStore } from "../../stores/app.store";
import { formatRelativeDate } from "../../utils/filePresentation";

/**
 * 通知中心：回看错过的提示。
 *
 * toast 会自动消失，这里保留最近的通知历史（`app.notificationHistory`），
 * 顶栏铃铛上的角标显示未读数量，打开面板即清零。
 */
const app = useAppStore();
const open = ref(false);
const rootRef = ref<HTMLElement | null>(null);

type NotificationFilter = "all" | "success" | "error" | "warning" | "info";

const filters: { value: NotificationFilter; label: string }[] = [
  { value: "all", label: "全部" },
  { value: "success", label: "成功" },
  { value: "error", label: "失败" },
  { value: "warning", label: "警告" },
  { value: "info", label: "信息" },
];

const activeFilter = ref<NotificationFilter>("all");

const visibleHistory = computed(() =>
  activeFilter.value === "all"
    ? app.notificationHistory
    : app.notificationHistory.filter(
        (item) => item.type === activeFilter.value,
      ),
);

const activeFilterLabel = computed(
  () => filters.find((item) => item.value === activeFilter.value)?.label ?? "",
);

const bellTitle = computed(() =>
  app.unreadNotifications > 0
    ? `通知（${app.unreadNotifications} 条未读）`
    : "通知",
);

function toggle() {
  open.value = !open.value;
  if (open.value) app.markNotificationsRead();
}

function onDocumentPointer(event: MouseEvent | TouchEvent) {
  if (!open.value) return;
  const target = event.target as Node | null;
  if (target && rootRef.value?.contains(target)) return;
  open.value = false;
}

function onKeydown(event: KeyboardEvent) {
  if (event.key === "Escape" && open.value) open.value = false;
}

onMounted(() => {
  document.addEventListener("click", onDocumentPointer, true);
  document.addEventListener("touchstart", onDocumentPointer, true);
  document.addEventListener("keydown", onKeydown);
});

onBeforeUnmount(() => {
  document.removeEventListener("click", onDocumentPointer, true);
  document.removeEventListener("touchstart", onDocumentPointer, true);
  document.removeEventListener("keydown", onKeydown);
});
</script>

<style scoped>
.app-bar-bell {
  position: relative;
}

.app-bar-bell.is-active {
  background: var(--vf-surface-hover);
}

/* 未读角标：与主流应用一致 */
.app-bar-bell-badge {
  position: absolute;
  top: -0.15rem;
  right: -0.15rem;
  min-width: 1rem;
  height: 1rem;
  padding: 0 0.2rem;
  border-radius: var(--vf-radius-pill);
  /* 语义配对（round 7 双主题 AA 验证）：原 danger-text 底 + 白字在深色下仅 ≈2.2:1；
     用 soft 底 / text 字并加强调色描边与字重，保持醒目且达标 */
  background: var(--vf-danger-soft);
  color: var(--vf-danger-text);
  border: 1px solid var(--vf-danger);
  font-weight: 700;
  font-size: 0.62rem;
  line-height: 1rem;
  text-align: center;
}

.notification-center-menu {
  min-width: min(20rem, calc(100vw - 2rem));
  padding: 0.25rem;
  border: 1px solid var(--vf-border-weak);
  border-radius: var(--vf-radius);
  /* 浮层色调抬升（同 ContextMenu）：深色下高于基准面一档 */
  background: var(--vf-surface-raised);
  box-shadow: var(--vf-shadow-menu);
}

.notification-center-panel {
  display: flex;
  flex-direction: column;
  max-height: 22rem;
}

.notification-center-head {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 0.35rem 0.5rem;
  border-bottom: 1px solid var(--vf-border-weak);
}

.notification-center-title {
  font-size: 0.82rem;
  font-weight: 600;
  color: var(--vf-text-strong);
}

.notification-center-clear {
  padding: 0;
  border: none;
  background: none;
  color: var(--vf-accent);
  font-size: 0.76rem;
  cursor: pointer;
}

.notification-center-clear:hover {
  text-decoration: underline;
}

.notification-center-filters {
  display: flex;
  align-items: center;
  gap: 0.25rem;
  flex-wrap: wrap;
  padding: 0.3rem 0.45rem;
  border-bottom: 1px solid var(--vf-border-weak);
}

.notification-center-filter {
  padding: 0.1rem 0.45rem;
  border: 1px solid var(--vf-border-weak);
  border-radius: var(--vf-radius-pill);
  background: var(--vf-surface);
  color: var(--vf-text-muted);
  font-size: 0.72rem;
  cursor: pointer;
}

.notification-center-filter:hover {
  background: var(--vf-surface-hover);
}

.notification-center-filter.is-active {
  border-color: var(--vf-accent);
  background: var(--vf-accent-soft);
  color: var(--vf-accent-text);
  font-weight: 600;
}

/* 单条移除：hover 时出现，避免平时过于喧闹 */
.notification-center-remove {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  flex: 0 0 auto;
  width: 1.4rem;
  height: 1.4rem;
  padding: 0;
  border: none;
  border-radius: var(--vf-radius-sm);
  background: transparent;
  color: var(--vf-text-subtle);
  cursor: pointer;
  opacity: 0;
}

.notification-center-item:hover .notification-center-remove,
.notification-center-remove:focus-visible {
  opacity: 1;
}

.notification-center-remove:hover {
  background: var(--vf-surface-hover);
  color: var(--vf-danger-text);
}

.notification-center-empty {
  margin: 0;
  padding: 1.25rem 0.5rem;
  color: var(--vf-text-subtle);
  font-size: 0.8rem;
  text-align: center;
}

.notification-center-list {
  display: flex;
  flex-direction: column;
  margin: 0;
  padding: 0.25rem;
  overflow-y: auto;
  list-style: none;
}

.notification-center-item {
  display: flex;
  align-items: flex-start;
  gap: 0.45rem;
  padding: 0.4rem 0.45rem;
  border-radius: var(--vf-radius-sm);
}

.notification-center-item:hover {
  background: var(--vf-surface-hover);
}

.notification-center-icon {
  display: inline-flex;
  flex: 0 0 auto;
  margin-top: 0.05rem;
}

.notification-center-item.is-success .notification-center-icon {
  color: var(--vf-success-text);
}

.notification-center-item.is-error .notification-center-icon {
  color: var(--vf-danger-text);
}

.notification-center-item.is-warning .notification-center-icon {
  color: var(--vf-warning-text);
}

.notification-center-item.is-info .notification-center-icon {
  color: var(--vf-accent);
}

.notification-center-body {
  min-width: 0;
}

.notification-center-message {
  margin: 0;
  color: var(--vf-text);
  font-size: 0.8rem;
  line-height: 1.5;
  word-break: break-word;
}

.notification-center-time {
  margin: 0.1rem 0 0;
  color: var(--vf-text-subtle);
  font-size: 0.7rem;
}
</style>
