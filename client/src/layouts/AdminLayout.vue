<template>
  <div class="admin-shell">
    <aside class="admin-sidebar" :class="{ 'is-open': drawerOpen }">
      <div class="admin-brand">
        <span class="admin-brand-mark" aria-hidden="true">
          <IconShieldLock :size="18" />
        </span>
        <div class="admin-brand-text">
          <strong>管理控制台</strong>
          <span>VFiles</span>
        </div>
        <button
          class="admin-drawer-close vf-icon-button"
          type="button"
          aria-label="关闭导航"
          @click="drawerOpen = false"
        >
          <IconX :size="16" />
        </button>
      </div>

      <nav class="admin-nav" aria-label="管理导航">
        <p class="admin-nav-label">系统管理</p>
        <RouterLink
          v-for="item in navItems"
          :key="item.key"
          :to="item.to"
          class="admin-nav-item"
          :class="{ 'is-active': active === item.key }"
          @click="drawerOpen = false"
        >
          <component :is="item.icon" :size="17" />
          <span>{{ item.label }}</span>
          <span v-if="active === item.key" class="admin-nav-indicator" aria-hidden="true" />
        </RouterLink>
      </nav>

      <div class="admin-sidebar-foot">
        <RouterLink class="admin-back-link" to="/">
          <IconArrowLeft :size="15" />
          <span>返回文件</span>
        </RouterLink>
      </div>
    </aside>

    <div
      v-if="drawerOpen"
      class="admin-drawer-scrim"
      aria-hidden="true"
      @click="drawerOpen = false"
    ></div>

    <div class="admin-workspace">
      <header class="admin-topbar">
        <div class="admin-topbar-left">
          <button
            class="admin-drawer-toggle vf-icon-button"
            type="button"
            aria-label="打开导航"
            @click="drawerOpen = true"
          >
            <IconMenu2 :size="18" />
          </button>
          <span class="admin-crumb">管理</span>
          <span class="admin-crumb-sep" aria-hidden="true">/</span>
          <span class="admin-crumb-current">{{ title }}</span>
        </div>
        <div class="admin-topbar-right">
          <ThemeToggle />
          <RouterLink
            class="vf-ghost-button admin-topbar-back"
            to="/"
            aria-label="文件"
          >
            <IconFolderOpen :size="15" />
            <span>文件</span>
          </RouterLink>
        </div>
      </header>

      <main class="admin-main">
        <div class="admin-main-inner">
          <PageHeader :title="title" :description="description">
            <template v-if="$slots.actions" #actions>
              <slot name="actions" />
            </template>
          </PageHeader>
          <slot />
        </div>
      </main>
    </div>
  </div>
</template>

<script setup lang="ts">
import { computed, ref } from "vue";
import {
  IconArrowLeft,
  IconFileText,
  IconFolderOpen,
  IconLayoutDashboard,
  IconMenu2,
  IconServer,
  IconShieldLock,
  IconUsers,
  IconX,
} from "@tabler/icons-vue";
import ThemeToggle from "../components/common/ThemeToggle.vue";
import { useAuthStore } from "../stores/auth.store";
import PageHeader from "../ui/PageHeader.vue";

defineProps<{
  title: string;
  description?: string;
  active: "overview" | "users" | "audit" | "system";
}>();

const drawerOpen = ref(false);
const authStore = useAuthStore();

const isAdmin = computed(() => authStore.user?.role === "admin");

const navItems = computed(() =>
  [
    { key: "overview", label: "概览", to: "/admin", icon: IconLayoutDashboard },
    { key: "users", label: "用户", to: "/admin/users", icon: IconUsers },
    ...(isAdmin.value
      ? [
          { key: "audit", label: "审计日志", to: "/admin/audit", icon: IconFileText },
          { key: "system", label: "系统", to: "/admin/system", icon: IconServer },
        ]
      : []),
  ] as const,
);
</script>

<style scoped>
.admin-shell {
  display: flex;
  min-height: 100vh;
  min-height: 100dvh;
  background: var(--vf-shell-bg);
}

.admin-sidebar {
  position: sticky;
  top: 0;
  display: flex;
  flex-direction: column;
  width: var(--vf-admin-sidebar-width);
  height: 100vh;
  height: 100dvh;
  flex: none;
  padding: var(--vf-space-2);
  background: var(--vf-shell-bg);
}

.admin-brand {
  display: flex;
  align-items: center;
  gap: var(--vf-space-2);
  padding: var(--vf-space-1) var(--vf-space-1) var(--vf-space-4);
}

.admin-brand-mark {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 34px;
  height: 34px;
  border-radius: var(--vf-radius-sm);
  background: var(--vf-accent-fill);
  color: var(--vf-on-accent);
}

.admin-brand-text {
  display: flex;
  flex-direction: column;
  line-height: 1.25;
}

.admin-brand-text strong {
  font-size: var(--vf-text-sm);
  color: var(--vf-text);
}

.admin-brand-text span {
  font-size: var(--vf-text-2xs);
  /* 小字在 shell 底上必须 ≥4.5:1：subtle 仅在白底达标（4.37 → 5.4）。 */
  color: var(--vf-text-muted);
}

.admin-drawer-close {
  display: none;
  margin-left: auto;
}

.admin-nav {
  display: flex;
  flex-direction: column;
  gap: var(--vf-space-1);
}

.admin-nav-label {
  margin: var(--vf-space-2) var(--vf-space-3) var(--vf-space-1);
  font-size: var(--vf-text-2xs);
  font-weight: 650;
  letter-spacing: 0.04em;
  text-transform: uppercase;
  color: var(--vf-text-muted);
}

.admin-nav-item {
  position: relative;
  display: flex;
  align-items: center;
  gap: var(--vf-space-2);
  min-height: var(--vf-sidebar-item-height);
  padding: 0 var(--vf-space-3);
  border-radius: var(--vf-radius-card);
  color: var(--vf-text-muted);
  font-size: var(--vf-text-base);
  font-weight: 500;
  text-decoration: none;
  transition: background-color 0.12s var(--vf-motion-standard),
    color 0.12s var(--vf-motion-standard);
}

.admin-nav-item:hover {
  background: var(--vf-row-hover);
  color: var(--vf-text);
}

.admin-nav-item.is-active {
  background: var(--vf-accent-fill);
  color: var(--vf-on-accent);
  font-weight: 600;
}

.admin-nav-indicator {
  position: absolute;
  left: calc(var(--vf-space-2) * -1);
  top: var(--vf-space-2);
  bottom: var(--vf-space-2);
  width: 2px;
  border-radius: 2px;
  background: var(--vf-accent);
}

.admin-sidebar-foot {
  margin-top: auto;
  padding-top: var(--vf-space-2);
  border-top: 1px solid var(--vf-border-weak);
}

.admin-back-link {
  display: flex;
  align-items: center;
  gap: var(--vf-space-2);
  min-height: var(--vf-sidebar-item-height);
  padding: 0 var(--vf-space-3);
  border-radius: var(--vf-radius-card);
  color: var(--vf-text-muted);
  font-size: var(--vf-text-sm);
  text-decoration: none;
}

.admin-back-link:hover {
  background: var(--vf-row-hover);
  color: var(--vf-text);
}

.admin-workspace {
  flex: 1;
  min-width: 0;
  display: flex;
  flex-direction: column;
}

.admin-topbar {
  position: sticky;
  top: 0;
  z-index: 5;
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: var(--vf-space-2);
  height: var(--vf-topbar-height);
  padding: 0 var(--vf-page-padding);
  background: var(--vf-shell-bg);
}

.admin-topbar-left,
.admin-topbar-right {
  display: flex;
  align-items: center;
  gap: var(--vf-space-2);
  min-width: 0;
}

.admin-drawer-toggle {
  display: none;
}

.admin-crumb {
  font-size: var(--vf-text-xs);
  color: var(--vf-text-muted);
}

.admin-crumb-sep {
  color: var(--vf-text-muted);
  font-size: var(--vf-text-xs);
}

.admin-crumb-current {
  font-size: var(--vf-text-sm);
  font-weight: 600;
  color: var(--vf-text);
}

/* 内容面板：与工作区四周留 8px 外壳间隙，内部页面直接落在面板上。 */
.admin-main {
  flex: 1 0 auto;
  margin: var(--vf-content-gap);
  border-radius: var(--vf-content-radius);
  background: var(--vf-content-bg);
  overflow: hidden;
}

.admin-main-inner {
  max-width: var(--vf-content-max);
  margin: 0 auto;
  padding: var(--vf-page-padding) var(--vf-page-padding) var(--vf-space-6);
  display: flex;
  flex-direction: column;
  gap: var(--vf-section-gap);
}

.admin-drawer-scrim {
  position: fixed;
  inset: 0;
  z-index: 40;
  background: var(--vf-overlay);
}

@media screen and (max-width: 1024px) {
  .admin-sidebar {
    position: fixed;
    left: 0;
    top: 0;
    z-index: 50;
    transform: translateX(-100%);
    transition: transform 0.18s var(--vf-motion-standard);
    box-shadow: var(--vf-shadow-popover);
  }

  .admin-sidebar.is-open {
    transform: translateX(0);
  }

  .admin-drawer-close,
  .admin-drawer-toggle {
    display: inline-flex;
  }

  .admin-main-inner {
    padding: var(--vf-page-padding) var(--vf-page-padding) var(--vf-space-6);
  }

  .admin-topbar {
    padding: 0 var(--vf-page-padding);
  }
}

@media screen and (max-width: 600px) {
  .admin-topbar-back span {
    display: none;
  }
}
</style>
