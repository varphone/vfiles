<template>
  <div ref="menuRef" class="file-toolbar-view-menu">
    <button
      class="file-toolbar-view-name"
      type="button"
      aria-haspopup="menu"
      :aria-expanded="open ? 'true' : 'false'"
      @click="emit('update:open', !open)"
    >
      <span>全部文件</span>
      <IconChevronDown
        class="file-toolbar-view-caret"
        :size="14"
        aria-hidden="true"
      />
    </button>

    <ul
      v-if="open"
      class="file-toolbar-view-list"
      role="menu"
      aria-label="切换到其它视图"
    >
      <li v-for="view in items" :key="view.to">
        <RouterLink
          class="file-toolbar-view-item"
          :class="{ 'is-active': route.path === view.to }"
          :to="view.to"
          role="menuitem"
          @click="emit('update:open', false)"
        >
          <component :is="view.icon" :size="16" aria-hidden="true" />
          <span>{{ view.label }}</span>
        </RouterLink>
      </li>
    </ul>
  </div>
</template>

<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref, type Component } from "vue";
import { useRoute } from "vue-router";
import {
  IconActivity,
  IconChevronDown,
  IconClock,
  IconFolder,
  IconShare,
  IconStar,
} from "@tabler/icons-vue";

const props = defineProps<{
  open: boolean;
  showShares: boolean;
}>();

const emit = defineEmits<{
  (e: "update:open", value: boolean): void;
}>();

const route = useRoute();
const menuRef = ref<HTMLElement | null>(null);

const items = computed<
  Array<{ key: string; to: string; label: string; icon: Component }>
>(() => {
  const entries: Array<{
    key: string;
    to: string;
    label: string;
    icon: Component;
  }> = [
    { key: "all", to: "/", label: "全部文件", icon: IconFolder },
    { key: "recent", to: "/recent", label: "最近", icon: IconClock },
    { key: "favorites", to: "/favorites", label: "收藏", icon: IconStar },
  ];
  if (props.showShares) {
    entries.push({
      key: "shares",
      to: "/shares",
      label: "分享",
      icon: IconShare,
    });
  }
  entries.push({
    key: "activity",
    to: "/activity",
    label: "活动",
    icon: IconActivity,
  });
  return entries;
});

function onDocumentPointer(event: MouseEvent | TouchEvent) {
  if (!props.open) return;
  const target = event.target as Node | null;
  if (target && menuRef.value && !menuRef.value.contains(target)) {
    emit("update:open", false);
  }
}

onMounted(() =>
  document.addEventListener("click", onDocumentPointer, true),
);
onBeforeUnmount(() =>
  document.removeEventListener("click", onDocumentPointer, true),
);
</script>

<style scoped>
/* 根视图的「全部文件 ⌄」视图名 + 视图切换菜单 */
.file-toolbar-view-menu {
  position: relative;
}

.file-toolbar-view-name {
  display: inline-flex;
  align-items: center;
  gap: var(--vf-space-1);
  height: var(--vf-control-height);
  padding: 0 var(--vf-space-2);
  border: none;
  border-radius: var(--vf-radius-control);
  background: transparent;
  color: var(--vf-text);
  font-family: inherit;
  font-size: var(--vf-text-lg);
  font-weight: 600;
  cursor: pointer;
}

.file-toolbar-view-name:hover,
.file-toolbar-view-name:focus-visible {
  background: var(--vf-row-hover);
}

.file-toolbar-view-caret {
  color: var(--vf-text-muted);
}

.file-toolbar-view-list {
  position: absolute;
  top: calc(100% + 0.3rem);
  left: 0;
  z-index: 30;
  min-width: 11rem;
  margin: 0;
  padding: var(--vf-space-1);
  list-style: none;
  border: 1px solid var(--vf-border);
  border-radius: var(--vf-radius-card);
  background: var(--vf-surface);
  box-shadow: var(--vf-shadow-menu);
}

.file-toolbar-view-item {
  display: flex;
  align-items: center;
  gap: var(--vf-space-3);
  height: var(--vf-control-height);
  padding: 0 var(--vf-space-3);
  border-radius: var(--vf-radius-control);
  color: var(--vf-text);
  font-size: var(--vf-text-base);
  text-decoration: none;
}

.file-toolbar-view-item:hover,
.file-toolbar-view-item:focus-visible {
  background: var(--vf-row-hover);
  color: var(--vf-text);
}

.file-toolbar-view-item.is-active {
  background: var(--vf-row-selected);
  color: var(--vf-accent-text);
}
</style>
