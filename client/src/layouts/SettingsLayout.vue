<template>
  <AppLayout>
    <div class="settings-layout">
      <!-- 移动端工具条：汉堡按钮打开设置抽屉（桌面隐藏）。 -->
      <div class="settings-mobile-bar">
        <button
          class="vf-icon-button settings-drawer-toggle"
          type="button"
          aria-label="打开设置导航"
          :aria-expanded="drawerOpen ? 'true' : 'false'"
          aria-controls="settings-sidebar"
          @click="openDrawer"
        >
          <IconMenu2 :size="20" />
        </button>
        <span v-if="title" class="settings-mobile-title">{{ title }}</span>
      </div>

      <aside
        id="settings-sidebar"
        class="settings-sidebar"
        :class="{ 'is-open': drawerOpen }"
        aria-label="设置导航"
      >
        <div class="settings-sidebar-head">
          <span class="settings-sidebar-head-title">设置</span>
          <button
            class="vf-icon-button settings-sidebar-close"
            type="button"
            aria-label="关闭设置导航"
            @click="closeDrawer"
          >
            <IconX :size="16" />
          </button>
        </div>

        <nav class="settings-sidebar-body" aria-label="设置">
          <h2 class="settings-group">设置</h2>
          <ul class="settings-nav">
            <li v-for="item in navItems" :key="item.key">
              <RouterLink
                class="settings-nav-item"
                :class="{ 'is-active': active === item.key }"
                :to="item.to"
                :aria-current="active === item.key ? 'page' : undefined"
                @click="closeDrawer"
              >
                <component :is="item.icon" :size="18" :stroke-width="1.7" />
                <span class="settings-nav-label">{{ item.label }}</span>
              </RouterLink>
            </li>
          </ul>

          <ul class="settings-nav settings-sidebar-footer">
            <li>
              <RouterLink class="settings-nav-item" to="/" @click="closeDrawer">
                <IconArrowLeft :size="18" :stroke-width="1.7" />
                <span class="settings-nav-label">返回文件</span>
              </RouterLink>
            </li>
          </ul>
        </nav>
      </aside>

      <div
        v-if="drawerOpen"
        class="settings-sidebar-scrim"
        aria-hidden="true"
        @click="closeDrawer"
      ></div>

      <main class="settings-content-panel">
        <div class="settings-page">
          <PageHeader :title="title" :description="description">
            <template v-if="$slots.actions" #actions>
              <slot name="actions" />
            </template>
          </PageHeader>
          <slot />
        </div>
      </main>
    </div>
  </AppLayout>
</template>

<script setup lang="ts">
import { onBeforeUnmount, onMounted, ref, watch } from "vue";
import { useRoute } from "vue-router";
import {
  IconArrowLeft,
  IconKey,
  IconMenu2,
  IconShare,
  IconTicket,
  IconX,
} from "@tabler/icons-vue";
import AppLayout from "./AppLayout.vue";
import PageHeader from "../ui/PageHeader.vue";

/**
 * 设置类页面骨架：与首页同构——顶栏 + 左侧 300px 设置栏 + 右侧圆角内容面板。
 *
 * 侧栏条目沿用首页视图条目（44px、8px 圆角、选中实心主色蓝底白字），
 * 移动端（≤ 768px）抽屉化，遮罩 / 关闭按钮 / Esc / 路由切换关闭。
 */
defineProps<{
  title: string;
  description?: string;
  active: "passwords" | "tokens" | "shares";
}>();

const navItems = [
  {
    key: "passwords",
    label: "访问密码",
    to: "/settings/passwords",
    icon: IconKey,
  },
  {
    key: "tokens",
    label: "访问令牌",
    to: "/settings/tokens",
    icon: IconTicket,
  },
  { key: "shares", label: "我的分享", to: "/shares", icon: IconShare },
] as const;

const route = useRoute();
const drawerOpen = ref(false);

function openDrawer() {
  drawerOpen.value = true;
}

function closeDrawer() {
  drawerOpen.value = false;
}

function onDocumentKeydown(event: KeyboardEvent) {
  if (event.key !== "Escape" || !drawerOpen.value) return;
  drawerOpen.value = false;
}

// 切换到别的页面后收起抽屉，避免停留在上一页的遮罩上。
watch(
  () => route.fullPath,
  () => {
    drawerOpen.value = false;
  },
);

onMounted(() => document.addEventListener("keydown", onDocumentKeydown));
onBeforeUnmount(() =>
  document.removeEventListener("keydown", onDocumentKeydown),
);
</script>

<style scoped>
.settings-layout {
  display: flex;
  flex: 1 1 auto;
  min-height: 0;
  overflow: hidden;
}

/* 移动端工具条：桌面不占位。 */
.settings-mobile-bar {
  display: none;
}

/*
 * 设置侧栏：与顶栏、页面背景共用外壳色（--vf-shell-bg），
 * 不再画分隔线，由内容面板的 8px 间隙界定两栏。
 */
.settings-sidebar {
  --vf-settings-item-radius: var(--vf-radius-card);
  --vf-settings-item-bg: var(--vf-surface);
  --vf-settings-item-hover: var(--vf-row-hover);

  display: flex;
  flex-direction: column;
  flex: 0 0 var(--vf-sidebar-width);
  width: var(--vf-sidebar-width);
  min-height: 0;
  background: var(--vf-shell-bg);
  overflow-y: auto;
}

.settings-sidebar-head {
  display: none;
}

.settings-sidebar-body {
  display: flex;
  flex-direction: column;
  flex: 1 1 auto;
  min-height: 0;
  /* 与条目同宽：分组标题、导航、返回入口共用 8px 外边距 */
  padding: var(--vf-space-2);
}

/* 分组标题：与首页「个人文件」同字号、同灰度，不是可点击条目。 */
.settings-group {
  margin: var(--vf-space-2) 0;
  padding: 0 var(--vf-space-3);
  color: var(--vf-text-muted);
  font-size: var(--vf-text-sm);
  font-weight: 600;
  letter-spacing: 0.01em;
  line-height: 1.5;
}

.settings-nav {
  display: flex;
  flex-direction: column;
  gap: var(--vf-space-1);
  margin: 0;
  padding: 0;
  list-style: none;
}

/* 条目：复用首页视图条目（44px、8px 圆角、浅色块）。 */
.settings-nav-item {
  display: flex;
  align-items: center;
  gap: var(--vf-space-2);
  width: 100%;
  height: var(--vf-sidebar-item-height);
  padding: 0 var(--vf-space-3);
  border: none;
  border-radius: var(--vf-settings-item-radius);
  background: var(--vf-settings-item-bg);
  color: var(--vf-text);
  font-family: inherit;
  font-size: var(--vf-text-base);
  font-weight: 500;
  text-align: left;
  text-decoration: none;
  white-space: nowrap;
}

.settings-nav-item:hover,
.settings-nav-item:focus-visible {
  background: var(--vf-settings-item-hover);
  color: var(--vf-text);
}

.settings-nav-item:focus-visible {
  outline: 2px solid var(--vf-accent);
  outline-offset: -2px;
}

/* 选中项：实心主色蓝底 + 白字，与首页视图栏一致。 */
.settings-nav-item.is-active {
  background: var(--vf-accent-fill);
  color: var(--vf-on-accent);
  font-weight: 600;
}

.settings-nav-item.is-active:hover,
.settings-nav-item.is-active:focus-visible {
  background: var(--vf-accent-fill);
  color: var(--vf-on-accent);
}

.settings-nav-label {
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
}

/* 返回文件固定在侧栏底部。 */
.settings-sidebar-footer {
  margin-top: auto;
  padding-top: var(--vf-space-2);
}

/*
 * 内容面板：比外壳浅一档的圆角矩形，四周留 8px 外壳间隙。
 * 页面在面板内部滚动，面板本身 overflow: hidden，圆角不会被内容顶破。
 */
.settings-content-panel {
  display: flex;
  flex-direction: column;
  flex: 1 1 auto;
  min-width: 0;
  min-height: 0;
  padding: var(--vf-page-padding);
  margin: var(--vf-content-gap);
  border-radius: var(--vf-content-radius);
  background: var(--vf-content-bg);
  overflow: hidden;
}

.settings-page {
  display: flex;
  flex-direction: column;
  flex: 1 1 auto;
  gap: var(--vf-section-gap);
  min-width: 0;
  min-height: 0;
  overflow-y: auto;
}

.settings-page :deep(.vf-page-header) {
  flex: 0 0 auto;
}

.settings-sidebar-scrim {
  display: none;
}

/* 移动端：侧栏抽屉化（遮罩 + 从左侧滑入）。 */
@media screen and (max-width: 768px) {
  .settings-layout {
    position: relative;
    display: block;
    overflow: visible;
  }

  .settings-mobile-bar {
    display: flex;
    align-items: center;
    gap: var(--vf-space-1);
    height: var(--vf-toolbar-height);
    padding: 0 var(--vf-space-2);
    background: var(--vf-shell-bg);
  }

  .settings-mobile-title {
    min-width: 0;
    overflow: hidden;
    white-space: nowrap;
    text-overflow: ellipsis;
    color: var(--vf-text);
    font-size: var(--vf-text-lg);
    font-weight: 600;
  }

  .settings-sidebar {
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

  .settings-sidebar.is-open {
    transform: translateX(0);
    visibility: visible;
    transition: transform 0.2s var(--vf-motion-standard);
  }

  .settings-sidebar-head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    flex: 0 0 auto;
    height: 44px;
    padding: 0 0.6rem;
    border-bottom: 1px solid var(--vf-border-weak);
  }

  .settings-sidebar-head-title {
    color: var(--vf-text-muted);
    font-size: var(--vf-text-sm);
    font-weight: 600;
  }

  /* 抽屉头已给出「设置」分组名，正文里不再重复同一标题。 */
  .settings-group {
    display: none;
  }

  .settings-sidebar-scrim {
    position: fixed;
    inset: var(--vf-topbar-height) 0 0;
    z-index: 60;
    display: block;
    background: var(--vf-overlay);
  }

  .settings-content-panel {
    margin: var(--vf-content-gap);
    padding: var(--vf-page-padding);
    overflow: visible;
  }

  .settings-page {
    overflow: visible;
  }
}

@media screen and (prefers-reduced-motion: reduce) {
  .settings-sidebar {
    transition: none;
  }
}
</style>
