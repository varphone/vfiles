<template>
  <Teleport to="body">
    <div
      v-if="show"
      ref="menuRef"
      class="vfiles-context-menu"
      :style="panelStyle"
      role="menu"
      @contextmenu.prevent
    >
      <button
        v-for="item in items"
        :key="item.key"
        class="vfiles-context-menu__item"
        :class="{ 'is-danger': item.danger }"
        type="button"
        role="menuitem"
        :disabled="item.disabled"
        @click="choose(item)"
      >
        <span
          v-if="item.icon"
          class="vfiles-context-menu__icon"
          aria-hidden="true"
        >
          <component :is="item.icon" :size="16" />
        </span>
        <span class="vfiles-context-menu__label">{{ item.label }}</span>
        <!-- 加速键标注（r121 ✓ Finder/Explorer/VSCode 菜单标配） -->
        <span v-if="item.shortcut" class="vfiles-context-menu__accel" aria-hidden="true">
          {{ item.shortcut }}
        </span>
      </button>
    </div>
  </Teleport>
</template>

<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref } from "vue";
import type { Component } from "vue";

export interface ContextMenuItem {
  key: string;
  label: string;
  /** 加速键标注（如 F2 / Del ✓ 与真实键绑定一致才标） */
  shortcut?: string;
  icon?: Component;
  danger?: boolean;
  disabled?: boolean;
}

const props = defineProps<{
  show: boolean;
  x: number;
  y: number;
  items: ContextMenuItem[];
}>();

const emit = defineEmits<{
  (e: "select", key: string): void;
  (e: "close"): void;
}>();

const menuRef = ref<HTMLElement | null>(null);
const MENU_WIDTH = 184;

const panelStyle = computed(() => ({
  left: `${Math.max(8, Math.min(props.x, window.innerWidth - MENU_WIDTH - 8))}px`,
  top: `${Math.max(8, Math.min(props.y, window.innerHeight - 8))}px`,
}));

function choose(item: ContextMenuItem) {
  if (item.disabled) return;
  emit("select", item.key);
  emit("close");
}

function onDocumentPointer(event: MouseEvent) {
  if (!props.show) return;
  const menu = menuRef.value;
  if (menu && event.target instanceof Node && menu.contains(event.target))
    return;
  emit("close");
}

function onKeydown(event: KeyboardEvent) {
  if (event.key === "Escape" && props.show) emit("close");
}

function onViewportChange() {
  if (props.show) emit("close");
}

function closeListeners() {
  document.removeEventListener("click", onDocumentPointer, true);
  document.removeEventListener("keydown", onKeydown);
  window.removeEventListener("resize", onViewportChange);
  window.removeEventListener("scroll", onViewportChange, true);
}

onMounted(() => {
  document.addEventListener("click", onDocumentPointer, true);
  document.addEventListener("keydown", onKeydown);
  window.addEventListener("resize", onViewportChange);
  window.addEventListener("scroll", onViewportChange, true);
});

onBeforeUnmount(closeListeners);
</script>

<style scoped>
.vfiles-context-menu {
  position: fixed;
  z-index: 60;
  min-width: 184px;
  padding: 4px;
  border: 1px solid var(--vf-border);
  border-radius: var(--vf-radius-sm);
  /* 浮层色调抬升：深色下比基准面高一档（M3 dark / GitHub dark 的 tonal elevation） */
  background: var(--vf-surface-raised);
  box-shadow: var(--vf-shadow-menu);
}

.vfiles-context-menu__item {
  display: flex;
  align-items: center;
  gap: 8px;
  width: 100%;
  padding: 8px 10px;
  border: none;
  border-radius: var(--vf-radius-sm);
  background: transparent;
  color: var(--vf-text);
  font-size: 0.875rem;
  text-align: left;
  cursor: pointer;
}

.vfiles-context-menu__item:hover:not(:disabled) {
  background: var(--vf-surface-sunken);
}

.vfiles-context-menu__item:disabled {
  color: var(--vf-text-subtle);
  cursor: not-allowed;
}

.vfiles-context-menu__item.is-danger {
  color: var(--vf-danger-text);
}

.vfiles-context-menu__item.is-danger:hover:not(:disabled) {
  background: var(--vf-danger-soft);
}

.vfiles-context-menu__icon {
  display: inline-flex;
}
</style>

.vfiles-context-menu__accel {
  margin-left: auto;
  padding-left: 1.2rem;
  color: var(--vf-text-muted);
  font-size: 0.75rem;
}
