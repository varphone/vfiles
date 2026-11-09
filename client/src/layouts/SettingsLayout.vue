<template>
  <div class="settings-shell">
    <div class="settings-bar">
      <RouterLink class="settings-back" to="/">
        <IconArrowLeft :size="15" />
        <span>返回文件</span>
      </RouterLink>
      <span class="settings-bar-sep" aria-hidden="true">/</span>
      <span class="settings-bar-title">设置</span>
      <div class="settings-bar-spacer"></div>
      <ThemeToggle />
    </div>

    <div class="settings-body">
      <aside class="settings-nav" aria-label="设置导航">
        <RouterLink
          v-for="item in navItems"
          :key="item.key"
          :to="item.to"
          class="settings-nav-item"
          :class="{ 'is-active': active === item.key }"
        >
          <component :is="item.icon" :size="16" />
          <span>{{ item.label }}</span>
        </RouterLink>
      </aside>

      <main class="settings-main">
        <PageHeader :title="title" :description="description">
          <template v-if="$slots.actions" #actions>
            <slot name="actions" />
          </template>
        </PageHeader>
        <slot />
      </main>
    </div>
  </div>
</template>

<script setup lang="ts">
import {
  IconArrowLeft,
  IconKey,
  IconShare,
  IconTicket,
} from "@tabler/icons-vue";
import ThemeToggle from "../components/common/ThemeToggle.vue";
import PageHeader from "../ui/PageHeader.vue";

defineProps<{
  title: string;
  description?: string;
  active: "passwords" | "tokens" | "shares";
}>();

const navItems = [
  { key: "passwords", label: "访问密码", to: "/settings/passwords", icon: IconKey },
  { key: "tokens", label: "访问令牌", to: "/settings/tokens", icon: IconTicket },
  { key: "shares", label: "我的分享", to: "/shares", icon: IconShare },
] as const;
</script>

<style scoped>
.settings-shell {
  min-height: 100dvh;
  background: var(--vf-canvas);
}

.settings-bar {
  position: sticky;
  top: 0;
  z-index: 5;
  display: flex;
  align-items: center;
  gap: 0.5rem;
  height: var(--vf-topbar-height);
  padding: 0 1.5rem;
  border-bottom: 1px solid var(--vf-border-weak);
  background: var(--vf-topbar);
  backdrop-filter: blur(10px);
}

.settings-back {
  display: inline-flex;
  align-items: center;
  gap: 0.35rem;
  color: var(--vf-text-muted);
  font-size: 0.8rem;
  text-decoration: none;
}

.settings-back:hover {
  color: var(--vf-accent-text);
}

.settings-bar-sep {
  color: var(--vf-text-subtle);
  font-size: 0.75rem;
}

.settings-bar-title {
  font-size: 0.86rem;
  font-weight: 650;
  color: var(--vf-text-strong);
}

.settings-bar-spacer {
  flex: 1;
}

.settings-body {
  display: flex;
  gap: 1.75rem;
  max-width: var(--vf-content-max);
  margin: 0 auto;
  padding: 1.5rem 1.5rem 3rem;
}

.settings-nav {
  position: sticky;
  top: calc(var(--vf-topbar-height) + 1.5rem);
  display: flex;
  flex-direction: column;
  gap: 0.15rem;
  width: var(--vf-settings-nav-width);
  flex: none;
  align-self: flex-start;
}

.settings-nav-item {
  display: flex;
  align-items: center;
  gap: 0.5rem;
  min-height: 36px;
  padding: 0.4rem 0.65rem;
  border-radius: var(--vf-radius-sm);
  color: var(--vf-text-muted);
  font-size: 0.84rem;
  font-weight: 500;
  text-decoration: none;
  transition: background-color 0.12s var(--vf-motion-standard),
    color 0.12s var(--vf-motion-standard);
}

.settings-nav-item:hover {
  background: var(--vf-surface-hover);
  color: var(--vf-text-strong);
}

.settings-nav-item.is-active {
  background: var(--vf-accent-soft);
  color: var(--vf-accent-text);
  font-weight: 600;
}

.settings-main {
  flex: 1;
  min-width: 0;
  display: flex;
  flex-direction: column;
  gap: 1.1rem;
}

@media screen and (max-width: 1024px) {
  .settings-body {
    flex-direction: column;
    gap: 1rem;
    padding: 1.1rem 1rem 2.5rem;
  }

  .settings-nav {
    position: static;
    width: 100%;
    flex-direction: row;
    overflow-x: auto;
    padding-bottom: 0.25rem;
  }

  .settings-nav-item {
    white-space: nowrap;
  }

  .settings-bar {
    padding: 0 1rem;
  }
}
</style>
