<template>
  <nav class="path-bar breadcrumb has-succeeds-separator" aria-label="路径导航">
    <ul class="path-bar-list">
      <li
        v-for="(crumb, index) in breadcrumbs"
        :key="crumb.path"
        class="path-bar-item"
        :class="{
          'is-active': index === breadcrumbs.length - 1,
          'drop-target': dropTarget === crumb.path,
        }"
      >
        <a
          class="path-bar-segment"
          href="#"
          :title="crumb.path ? `/${crumb.path}` : '根目录'"
          @click.prevent="go(crumb.path)"
          @dragover.prevent="onDragOver(crumb.path)"
          @dragleave="onDragLeave(crumb.path)"
          @drop.prevent="onDrop(crumb.path)"
        >
          <IconHome v-if="index === 0" :size="16" class="path-bar-icon" />
          <span>{{ crumb.name }}</span>
        </a>

        <button
          v-if="index === breadcrumbs.length - 1"
          :ref="registerToggle"
          class="path-bar-toggle"
          type="button"
          :aria-expanded="open ? 'true' : 'false'"
          aria-label="展开当前目录的子文件夹"
          @click.stop="toggle"
        >
          <IconChevronDown :size="14" />
        </button>

        <!--
          菜单挂到 body：面包屑列表为了横向滚动设置了 overflow-x: auto，
          绝对定位的下拉会被这个滚动容器裁剪（表现为「点了没反应」）。
        -->
        <Teleport v-if="index === breadcrumbs.length - 1 && open" to="body">
          <div ref="menuRef" class="path-bar-menu" :style="menuStyle">
            <BreadcrumbTreeMenu
              :current-path="crumb.path"
              :directories="directories"
              @navigate="go"
            />
          </div>
        </Teleport>
      </li>
    </ul>
  </nav>
</template>

<script setup lang="ts">
import { onBeforeUnmount, onMounted, ref } from "vue";
import { IconChevronDown, IconHome } from "@tabler/icons-vue";
import BreadcrumbTreeMenu from "./BreadcrumbTreeMenu.vue";
import type { FileInfo } from "../../types";

withDefaults(
  defineProps<{
    breadcrumbs: Array<{ name: string; path: string }>;
    /** 当前目录下的子文件夹，用于快速跳转。 */
    directories?: FileInfo[];
  }>(),
  {
    directories: () => [],
  },
);

const emit = defineEmits<{
  (e: "navigate", path: string): void;
  (e: "drop", path: string): void;
}>();

const open = ref(false);
const dropTarget = ref<string | null>(null);
const toggleEl = ref<HTMLElement | null>(null);
/** 菜单固定定位（相对视口），打开时按按钮位置计算。 */
const menuStyle = ref<Record<string, string>>({});

function registerToggle(element: unknown) {
  toggleEl.value = element instanceof HTMLElement ? element : null;
}

function updateMenuPosition() {
  const rect = toggleEl.value?.getBoundingClientRect();
  if (!rect) return;
  // 贴右边缘时向左收，避免菜单超出视口
  const left = Math.max(8, Math.min(rect.left, window.innerWidth - 232));
  menuStyle.value = {
    position: "fixed",
    top: `${Math.round(rect.bottom + 6)}px`,
    left: `${Math.round(left)}px`,
  };
}

function toggle() {
  if (!open.value) updateMenuPosition();
  open.value = !open.value;
}

function closeMenu() {
  open.value = false;
}

function go(path: string) {
  open.value = false;
  emit("navigate", path);
}

/** 拖放：路径段可作为放置目标，把条目移动到该目录。 */
function onDragOver(path: string) {
  dropTarget.value = path;
}

function onDragLeave(path: string) {
  if (dropTarget.value === path) dropTarget.value = null;
}

function onDrop(path: string) {
  dropTarget.value = null;
  emit("drop", path);
}

function onDocumentClick(event: MouseEvent) {
  if (!open.value) return;
  const target = event.target;
  if (target instanceof Element) {
    // 菜单已 Teleport 到 body，需要单独判断
    if (target.closest(".path-bar-menu") !== null) return;
    if (target.closest(".path-bar") !== null) return;
  }
  open.value = false;
}

function onKeydown(event: KeyboardEvent) {
  if (event.key === "Escape" && open.value) open.value = false;
}

onMounted(() => {
  document.addEventListener("click", onDocumentClick, true);
  document.addEventListener("keydown", onKeydown);
  // 视口变化时菜单位置会失效，直接关闭更稳妥
  window.addEventListener("resize", closeMenu);
  window.addEventListener("scroll", closeMenu, true);
});

onBeforeUnmount(() => {
  document.removeEventListener("click", onDocumentClick, true);
  document.removeEventListener("keydown", onKeydown);
  window.removeEventListener("resize", closeMenu);
  window.removeEventListener("scroll", closeMenu, true);
});
</script>

<style scoped>
.path-bar {
  display: flex;
  align-items: center;
  min-height: 2.1rem;
  margin: 0;
  padding: 0;
}

.path-bar-list {
  display: flex;
  align-items: center;
  flex-wrap: nowrap;
  gap: 2px;
  margin: 0;
  padding: 0;
  list-style: none;
  overflow-x: auto;
  scrollbar-width: thin;
}

.path-bar-item {
  position: relative;
  display: flex;
  align-items: center;
  flex: 0 0 auto;
}

.path-bar-segment {
  display: inline-flex;
  align-items: center;
  gap: 4px;
  max-width: 16rem;
  padding: 3px 8px;
  border-radius: var(--vf-radius-sm);
  color: var(--vf-text-strong);
  font-size: 0.9rem;
  font-weight: 600;
  text-decoration: none;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

.path-bar-segment:hover {
  background: var(--vf-accent-tint);
  color: var(--vf-accent);
}

.path-bar-item.is-active .path-bar-segment {
  color: var(--vf-text-strong);
  cursor: default;
}

.path-bar-icon {
  flex: 0 0 auto;
}

.path-bar-toggle {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 22px;
  height: 22px;
  margin-left: 1px;
  border: none;
  border-radius: var(--vf-radius-sm);
  background: transparent;
  color: var(--vf-text-muted);
  cursor: pointer;
}

.path-bar-toggle:hover {
  background: var(--vf-accent-tint);
  color: var(--vf-accent);
}

.path-bar-menu {
  z-index: 60;
  min-width: 190px;
  padding: 4px;
  border: 1px solid var(--vf-border);
  border-radius: var(--vf-radius);
  background: var(--vf-surface);
  box-shadow: var(--vf-shadow-menu);
}

@media screen and (max-width: 1023px) {
  .path-bar-segment {
    max-width: 9rem;
    font-size: 0.82rem;
  }
}
</style>
