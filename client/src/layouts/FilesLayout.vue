<template>
  <AppLayout>
    <div class="files-layout">
      <!-- 移动端工具条：汉堡按钮打开视图抽屉（桌面隐藏）。 -->
      <div class="files-mobile-bar">
        <button
          class="vf-icon-button files-drawer-toggle"
          type="button"
          aria-label="打开视图栏"
          :aria-expanded="isOpen ? 'true' : 'false'"
          aria-controls="files-view-sidebar"
          @click="openSidebar"
        >
          <IconMenu2 :size="20" />
        </button>
        <span v-if="title" class="files-mobile-title">{{ title }}</span>
      </div>

      <aside
        id="files-view-sidebar"
        class="files-sidebar"
        :class="{ 'is-open': isOpen }"
        aria-label="文件视图"
      >
        <div class="files-sidebar-head">
          <span class="files-sidebar-head-title">视图</span>
          <button
            class="vf-icon-button files-sidebar-close"
            type="button"
            aria-label="关闭视图栏"
            @click="closeSidebar"
          >
            <IconX :size="16" />
          </button>
        </div>
        <div class="files-sidebar-body">
          <slot name="sidebar" />
        </div>
      </aside>

      <div
        v-if="isOpen"
        class="files-sidebar-scrim"
        aria-hidden="true"
        @click="closeSidebar"
      ></div>

      <div class="files-content">
        <slot />
      </div>
    </div>
  </AppLayout>
</template>

<script setup lang="ts">
import { onBeforeUnmount, onMounted, ref, watch } from "vue";
import { useRoute } from "vue-router";
import { IconMenu2, IconX } from "@tabler/icons-vue";
import AppLayout from "./AppLayout.vue";

/**
 * 文件页两栏骨架：左侧视图栏（`--vf-sidebar-width`）+ 内容区。
 *
 * 移动端（≤768px）视图栏变成抽屉，由单一 `isOpen` 布尔控制：汉堡按钮打开，
 * 遮罩 / 关闭按钮 / Esc / 路由切换关闭。`isOpen` 通过 defineExpose 暴露，
 * 父级也能主动控制抽屉状态。
 */
withDefaults(
  defineProps<{
    /** 移动端工具条上的当前视图名。 */
    title?: string;
  }>(),
  { title: "" },
);

const route = useRoute();

const isOpen = ref(false);

function openSidebar() {
  isOpen.value = true;
}

function closeSidebar() {
  isOpen.value = false;
}

function toggleSidebar() {
  isOpen.value = !isOpen.value;
}

function onDocumentKeydown(event: KeyboardEvent) {
  if (event.key !== "Escape" || !isOpen.value) return;
  closeSidebar();
}

// 切换到别的视图后收起抽屉，避免停留在上一页的遮罩上。
watch(
  () => route.fullPath,
  () => closeSidebar(),
);

onMounted(() => document.addEventListener("keydown", onDocumentKeydown));
onBeforeUnmount(() =>
  document.removeEventListener("keydown", onDocumentKeydown),
);

defineExpose({ isOpen, openSidebar, closeSidebar, toggleSidebar });
</script>

<style scoped>
.files-layout {
  display: flex;
  flex: 1 1 auto;
  min-height: 0;
  overflow: hidden;
}

/* 移动端工具条：桌面不占位。 */
.files-mobile-bar {
  display: none;
}

.files-sidebar {
  display: flex;
  flex-direction: column;
  flex: 0 0 var(--vf-sidebar-width);
  width: var(--vf-sidebar-width);
  min-height: 0;
  background: var(--vf-sidebar);
  border-right: 1px solid var(--vf-border-weak);
  overflow-y: auto;
}

.files-sidebar-head {
  display: none;
}

.files-sidebar-body {
  display: flex;
  flex-direction: column;
  flex: 1 1 auto;
  min-height: 0;
}

.files-content {
  display: flex;
  flex-direction: column;
  flex: 1 1 auto;
  min-width: 0;
  min-height: 0;
  padding: 1.25rem 1.5rem;
  overflow-y: auto;
}

.files-sidebar-scrim {
  display: none;
}

/* 移动端：视图栏抽屉化（遮罩 + 从左侧滑入）。 */
@media screen and (max-width: 768px) {
  .files-layout {
    position: relative;
    display: block;
    overflow: visible;
  }

  .files-mobile-bar {
    display: flex;
    align-items: center;
    gap: 0.4rem;
    height: 48px;
    padding: 0 0.5rem;
    border-bottom: 1px solid var(--vf-border-weak);
    background: var(--vf-surface);
  }

  .files-mobile-title {
    min-width: 0;
    overflow: hidden;
    white-space: nowrap;
    text-overflow: ellipsis;
    color: var(--vf-text-strong);
    font-size: 0.95rem;
    font-weight: 600;
  }

  .files-sidebar {
    position: fixed;
    top: var(--vf-topbar-height);
    bottom: 0;
    left: 0;
    z-index: 61;
    width: min(var(--vf-sidebar-width), 84vw);
    flex: 0 0 auto;
    transform: translateX(-100%);
    /* 收起时对辅助技术隐藏，避免 Tab 进入屏幕外的抽屉内容 */
    visibility: hidden;
    transition:
      transform 0.2s var(--vf-motion-standard),
      visibility 0s linear 0.2s;
    box-shadow: var(--vf-shadow-menu);
  }

  .files-sidebar.is-open {
    transform: translateX(0);
    visibility: visible;
    transition: transform 0.2s var(--vf-motion-standard);
  }

  .files-sidebar-head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    height: 44px;
    padding: 0 0.6rem;
    border-bottom: 1px solid var(--vf-border-weak);
  }

  .files-sidebar-head-title {
    color: var(--vf-text-muted);
    font-size: 0.8rem;
    font-weight: 600;
  }

  .files-sidebar-scrim {
    position: fixed;
    inset: var(--vf-topbar-height) 0 0;
    z-index: 60;
    display: block;
    background: var(--vf-overlay);
  }

  .files-content {
    padding: 0.75rem;
    overflow-y: visible;
  }
}

@media screen and (prefers-reduced-motion: reduce) {
  .files-sidebar {
    transition: none;
  }
}
</style>
