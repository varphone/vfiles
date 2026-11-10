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
        :class="{ 'is-open': isOpen, 'is-collapsed': isCollapsed }"
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
import { computed, onBeforeUnmount, onMounted, ref, watch } from "vue";
import { useRoute } from "vue-router";
import { IconMenu2, IconX } from "@tabler/icons-vue";
import AppLayout from "./AppLayout.vue";
import { provideFilesSidebarControl } from "../composables/useFilesSidebar";
import { useMediaQuery } from "../composables/useMediaQuery";

/**
 * 文件页两栏骨架：左侧视图栏（`--vf-sidebar-width` = 300px）+ 内容区。
 *
 * 侧栏两套开合语义：
 * - 桌面（> 768px）：常驻，可被文件工具栏的汉堡按钮收起/展开（`isCollapsed`）；
 * - 移动端（≤ 768px）：变成抽屉（`isOpen`），由移动端工具条打开，
 *   遮罩 / 关闭按钮 / Esc / 路由切换关闭。
 * 两种状态都通过 provide 暴露给文件工具栏（见 `useFilesSidebar`），
 * 这样 Home 不必把回调一路透传给 FileBrowser。
 */
withDefaults(
  defineProps<{
    /** 移动端工具条上的当前视图名。 */
    title?: string;
  }>(),
  { title: "" },
);

const route = useRoute();
const isMobile = useMediaQuery("(max-width: 768px)");

/** 移动端抽屉是否展开。 */
const isOpen = ref(false);
/** 桌面端侧栏是否被收起。 */
const isCollapsed = ref(false);

/** 侧栏当前是否可见：桌面看收起态，移动看抽屉。 */
const isSidebarVisible = computed(() =>
  isMobile.value ? isOpen.value : !isCollapsed.value,
);

function openSidebar() {
  if (isMobile.value) {
    isOpen.value = true;
    return;
  }
  isCollapsed.value = false;
}

function closeSidebar() {
  if (isMobile.value) {
    isOpen.value = false;
    return;
  }
  isCollapsed.value = true;
}

/** 文件工具栏汉堡按钮的统一入口：桌面收起/展开侧栏，移动打开/关闭抽屉。 */
function toggleSidebar() {
  if (isMobile.value) {
    isOpen.value = !isOpen.value;
    return;
  }
  isCollapsed.value = !isCollapsed.value;
}

provideFilesSidebarControl({ isVisible: isSidebarVisible, toggle: toggleSidebar });

function onDocumentKeydown(event: KeyboardEvent) {
  if (event.key !== "Escape" || !isOpen.value) return;
  isOpen.value = false;
}

// 切换到别的视图后收起抽屉，避免停留在上一页的遮罩上。
// 桌面收起态属于用户偏好，路由切换不重置。
watch(
  () => route.fullPath,
  () => {
    isOpen.value = false;
  },
);

onMounted(() => document.addEventListener("keydown", onDocumentKeydown));
onBeforeUnmount(() =>
  document.removeEventListener("keydown", onDocumentKeydown),
);

defineExpose({
  isOpen,
  isCollapsed,
  isSidebarVisible,
  openSidebar,
  closeSidebar,
  toggleSidebar,
});
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

/*
 * 视图侧栏：与顶栏、页面背景共用外壳色（--vf-shell-bg），
 * 不再画渐变或右侧分隔线，由内容面板的 8px 间隙界定两栏。
 */
.files-sidebar {
  display: flex;
  flex-direction: column;
  flex: 0 0 var(--vf-sidebar-width);
  width: var(--vf-sidebar-width);
  min-height: 0;
  background: var(--vf-shell-bg);
  overflow-y: auto;
}

/* 桌面端收起：整列让出宽度（汉堡按钮再从工具栏展开）。 */
@media screen and (min-width: 769px) {
  .files-sidebar.is-collapsed {
    display: none;
  }
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

/*
 * 内容面板：比外壳浅一档的圆角矩形，四周留 8px 外壳间隙。
 * 滚动交给面板内部（文件列表 / 视图页各自滚动），
 * 面板本身 overflow: hidden，圆角不会被内部内容顶破。
 */
.files-content {
  display: flex;
  flex-direction: column;
  flex: 1 1 auto;
  min-width: 0;
  min-height: 0;
  /* 全站内容左右基线 16px（文件页由列表自己铺满，见 Home.vue） */
  padding: var(--vf-page-padding);
  margin: var(--vf-content-gap);
  border-radius: var(--vf-content-radius);
  background: var(--vf-content-bg);
  overflow: hidden;
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
    gap: var(--vf-space-1);
    height: var(--vf-toolbar-height);
    padding: 0 var(--vf-space-2);
    background: var(--vf-shell-bg);
  }

  .files-mobile-title {
    min-width: 0;
    overflow: hidden;
    white-space: nowrap;
    text-overflow: ellipsis;
    color: var(--vf-text);
    font-size: var(--vf-text-lg);
    font-weight: 600;
  }

  .files-sidebar {
    position: fixed;
    top: var(--vf-topbar-height);
    bottom: 0;
    padding-bottom: calc(var(--vf-space-2) + env(safe-area-inset-bottom));
    overscroll-behavior: contain;
    left: 0;
    z-index: var(--vf-z-drawer);
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
    font-size: var(--vf-text-sm);
    font-weight: 600;
  }

  .files-sidebar-scrim {
    position: fixed;
    inset: var(--vf-topbar-height) 0 0;
    z-index: var(--vf-z-scrim);
    display: block;
    background: var(--vf-overlay);
  }

  .files-content {
    margin: var(--vf-content-gap);
    padding: var(--vf-page-padding);
    overflow: visible;
  }
}

@media screen and (prefers-reduced-motion: reduce) {
  .files-sidebar {
    transition: none;
  }
}
</style>
