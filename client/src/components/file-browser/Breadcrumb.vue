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
          <IconHome v-if="index === 0" :size="15" class="path-bar-icon" />
          <span>{{ crumb.name }}</span>
        </a>

        <button
          v-if="index === breadcrumbs.length - 1 && directories.length > 0"
          class="path-bar-toggle"
          type="button"
          :aria-expanded="open ? 'true' : 'false'"
          aria-label="展开当前目录的子文件夹"
          @click.stop="toggle"
        >
          <IconChevronDown :size="14" />
        </button>

        <div
          v-if="index === breadcrumbs.length - 1 && open"
          ref="menuRef"
          class="path-bar-menu dropdown-content"
          role="menu"
        >
          <a
            v-for="directory in directories"
            :key="directory.path"
            class="dropdown-item path-bar-menu-item"
            href="#"
            role="menuitem"
            @click.prevent="go(directory.path)"
          >
            <IconFolder :size="15" class="mr-2" />
            <span>{{ directory.name }}</span>
          </a>
        </div>
      </li>
    </ul>
  </nav>
</template>

<script setup lang="ts">
import { onBeforeUnmount, onMounted, ref } from "vue";
import { IconChevronDown, IconFolder, IconHome } from "@tabler/icons-vue";
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
const dropTarget = ref("");

function toggle() {
  open.value = !open.value;
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
  if (dropTarget.value === path) dropTarget.value = "";
}

function onDrop(path: string) {
  dropTarget.value = "";
  emit("drop", path);
}

function onDocumentClick(event: MouseEvent) {
  if (!open.value) return;
  const target = event.target;
  if (target instanceof Element && target.closest(".path-bar") !== null) {
    return;
  }
  open.value = false;
}

function onKeydown(event: KeyboardEvent) {
  if (event.key === "Escape" && open.value) open.value = false;
}

onMounted(() => {
  document.addEventListener("click", onDocumentClick, true);
  document.addEventListener("keydown", onKeydown);
});

onBeforeUnmount(() => {
  document.removeEventListener("click", onDocumentClick, true);
  document.removeEventListener("keydown", onKeydown);
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
  border-radius: 8px;
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
  border-radius: 6px;
  background: transparent;
  color: var(--vf-text-muted);
  cursor: pointer;
}

.path-bar-toggle:hover {
  background: var(--vf-accent-tint);
  color: var(--vf-accent);
}

.path-bar-menu {
  position: absolute;
  top: calc(100% + 6px);
  left: 0;
  z-index: 40;
  min-width: 190px;
  max-height: 320px;
  overflow-y: auto;
  padding: 4px;
  border: 1px solid var(--vf-border);
  border-radius: 10px;
  background: var(--vf-surface);
  box-shadow: var(--vf-shadow-menu);
}

.path-bar-menu-item {
  display: flex;
  align-items: center;
  border-radius: 6px;
  font-size: 0.82rem;
}

@media screen and (max-width: 1023px) {
  .path-bar-segment {
    max-width: 9rem;
    font-size: 0.82rem;
  }
}
</style>
