<template>
  <header class="app-header" role="banner">
    <div class="app-header-inner">
      <div class="app-header-left">
        <RouterLink class="app-header-brand" to="/" aria-label="VFiles 首页">
          <img class="app-header-logo" src="/vfiles-icon.svg" alt="" />
          <span class="app-header-brand-name">VFiles</span>
        </RouterLink>

        <nav class="app-header-apps" aria-label="应用">
          <RouterLink
            v-for="app in apps"
            :key="app.key"
            class="app-header-app"
            :class="{ 'is-active': isActiveApp(app.to) }"
            :to="app.to"
            :title="app.label"
            :aria-label="app.label"
            :aria-current="isActiveApp(app.to) ? 'page' : undefined"
          >
            <component :is="app.icon" :size="20" :stroke-width="1.7" />
          </RouterLink>
        </nav>

        <div v-if="$slots.center" class="app-header-center">
          <slot name="center" />
        </div>
      </div>

      <div class="app-header-actions">
        <button
          class="vf-icon-button app-header-search"
          type="button"
          title="搜索（Ctrl+K）"
          aria-label="搜索"
          aria-keyshortcuts="Control+K Meta+K"
          @click="openSearch"
        >
          <IconSearch :size="19" />
        </button>

        <NotificationCenter />
        <ThemeToggle />

        <div
          ref="accountMenuRef"
          class="dropdown is-right app-header-account-menu"
          :class="{ 'is-active': accountMenuOpen }"
        >
          <div class="dropdown-trigger">
            <button
              class="app-header-account"
              type="button"
              aria-haspopup="true"
              :aria-expanded="accountMenuOpen ? 'true' : 'false'"
              :title="accountName"
              aria-label="账号菜单"
              @click="accountMenuOpen = !accountMenuOpen"
            >
              <span class="app-header-avatar" aria-hidden="true">
                <IconUser v-if="!accountInitial" :size="14" />
                <template v-else>{{ accountInitial }}</template>
              </span>
              <span class="app-header-account-name">{{ accountName }}</span>
              <IconChevronDown :size="14" class="app-header-account-caret" />
            </button>
          </div>

          <div class="dropdown-menu" role="menu">
            <div class="dropdown-content">
              <div class="dropdown-item app-header-account-summary">
                <p class="app-header-account-caption">账号</p>
                <p class="app-header-account-profile">{{ profileLabel }}</p>
              </div>

              <hr class="dropdown-divider" />

              <RouterLink
                v-if="showAccountEntries && auth.user && shareEnabled"
                class="dropdown-item"
                to="/shares"
                @click="closeAccountMenu"
              >
                我的分享
              </RouterLink>

              <RouterLink
                v-if="showAccountEntries && auth.enabled && auth.user"
                class="dropdown-item"
                to="/settings/passwords"
                @click="closeAccountMenu"
              >
                访问密码
              </RouterLink>

              <RouterLink
                v-if="showAccountEntries && auth.enabled && auth.user"
                class="dropdown-item"
                to="/settings/tokens"
                @click="closeAccountMenu"
              >
                访问令牌
              </RouterLink>

              <RouterLink
                v-if="
                  showAccountEntries &&
                  auth.enabled &&
                  ['admin', 'manager'].includes(auth.user?.role || '')
                "
                class="dropdown-item"
                to="/admin"
                @click="closeAccountMenu"
              >
                管理控制台
              </RouterLink>

              <RouterLink
                v-if="auth.enabled && !auth.user"
                class="dropdown-item"
                to="/login"
                @click="closeAccountMenu"
              >
                去登录
              </RouterLink>

              <a
                v-else-if="auth.enabled && auth.user"
                class="dropdown-item"
                href="#"
                @click.prevent="logoutFromMenu"
              >
                退出登录
              </a>
            </div>
          </div>
        </div>
      </div>
    </div>

    <UnifiedSearch :show="searchOpen" @close="closeSearch" />
  </header>
</template>

<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref, type Component } from "vue";
import { useRoute, useRouter } from "vue-router";
import {
  IconActivity,
  IconChevronDown,
  IconFolder,
  IconSearch,
  IconSettings,
  IconShare,
  IconUser,
} from "@tabler/icons-vue";
import NotificationCenter from "../common/NotificationCenter.vue";
import ThemeToggle from "../common/ThemeToggle.vue";
import UnifiedSearch from "./UnifiedSearch.vue";
import { useAppStore } from "../../stores/app.store";
import { useAuthStore } from "../../stores/auth.store";

/**
 * Nextcloud 式顶栏：左侧品牌 + 应用图标组，右侧统一搜索、通知、主题与账号。
 *
 * 应用图标只按路由高亮当前应用；「文件」在文件类视图（/、/recent、/favorites）
 * 下同样保持高亮，避免进入子视图后顶栏失去定位。
 * 页面特有控件（如首页的版本切换）通过 `#center` 插槽注入。
 */
type AppEntry = {
  key: string;
  to: string;
  label: string;
  icon: Component;
};

/** 归属「文件」应用的路由：子视图仍高亮文件图标。 */
const FILE_VIEW_PATHS = new Set(["/", "/recent", "/favorites"]);

const auth = useAuthStore();
const appStore = useAppStore();
const route = useRoute();
const router = useRouter();

const accountMenuOpen = ref(false);
const accountMenuRef = ref<HTMLElement | null>(null);
const searchOpen = ref(false);

const shareEnabled = computed(() => auth.features?.shareEnabled !== false);

const apps = computed<AppEntry[]>(() => {
  const entries: AppEntry[] = [
    { key: "files", to: "/", label: "文件", icon: IconFolder },
    { key: "activity", to: "/activity", label: "活动", icon: IconActivity },
  ];

  // 分享与文件范围绑定：受限会话看不到分享入口（路由守卫也会回首页）。
  if (!auth.isRestricted && shareEnabled.value) {
    entries.push({
      key: "shares",
      to: "/shares",
      label: "分享",
      icon: IconShare,
    });
  }

  // 管理入口只对管理角色开放，受限会话一律隐藏。
  if (
    !auth.isRestricted &&
    ["admin", "manager"].includes(auth.user?.role || "")
  ) {
    entries.push({
      key: "admin",
      to: "/admin",
      label: "管理",
      icon: IconSettings,
    });
  }

  return entries;
});

/** 账号级入口（凭据 / 令牌 / 分享 / 管理）在受限会话里不展示。 */
const showAccountEntries = computed(() => !auth.isRestricted);

const accountName = computed(() => auth.user?.username || "账号");

const accountInitial = computed(() => {
  const name = auth.user?.username || "";
  return name ? name.slice(0, 1).toUpperCase() : "";
});

const profileLabel = computed(() => {
  if (!auth.enabled) return "本地模式";
  if (!auth.user) return "认证已启用";
  return `${auth.user.username} · ${auth.user.role}`;
});

function isActiveApp(to: string): boolean {
  if (to === "/") return FILE_VIEW_PATHS.has(route.path);
  return route.path === to || route.path.startsWith(`${to}/`);
}

function closeAccountMenu() {
  accountMenuOpen.value = false;
}

function openSearch() {
  closeAccountMenu();
  searchOpen.value = true;
}

function closeSearch() {
  searchOpen.value = false;
}

async function logoutFromMenu() {
  closeAccountMenu();
  try {
    await auth.logout();
  } finally {
    appStore.info("已退出登录");
    void router.push({ name: "login", query: { redirect: "/" } });
  }
}

/**
 * 统一搜索的全局快捷键：Ctrl/Cmd+K。
 * 在 document 上注册（输入框内同样生效），卸载时移除，避免路由切换后重复监听。
 */
function onGlobalKeydown(event: KeyboardEvent) {
  if (!(event.ctrlKey || event.metaKey)) return;
  if (event.key.toLowerCase() !== "k") return;
  event.preventDefault();
  closeAccountMenu();
  searchOpen.value = true;
}

function onDocumentPointer(event: MouseEvent | TouchEvent) {
  if (!accountMenuOpen.value) return;
  const target = event.target as Node | null;
  if (target && accountMenuRef.value?.contains(target)) return;
  accountMenuOpen.value = false;
}

function onDocumentKeydown(event: KeyboardEvent) {
  if (event.key !== "Escape") return;
  accountMenuOpen.value = false;
}

onMounted(() => {
  document.addEventListener("keydown", onGlobalKeydown);
  document.addEventListener("click", onDocumentPointer, true);
  document.addEventListener("touchstart", onDocumentPointer, true);
  document.addEventListener("keydown", onDocumentKeydown);
});

onBeforeUnmount(() => {
  document.removeEventListener("keydown", onGlobalKeydown);
  document.removeEventListener("click", onDocumentPointer, true);
  document.removeEventListener("touchstart", onDocumentPointer, true);
  document.removeEventListener("keydown", onDocumentKeydown);
});
</script>

<style scoped>
/*
 * 顶栏：50px 实心主色蓝（Nextcloud 外壳），无下边框；
 * 文字与图标一律白色（--vf-on-accent），悬停/选中用半透明白色状态层。
 */
.app-header {
  /* 顶栏状态层：悬停 12% / 当前应用 18%（Nextcloud 应用图标的选中块）。 */
  --vf-header-hover: rgba(255, 255, 255, 0.12);
  --vf-header-active: rgba(255, 255, 255, 0.18);
  --vf-header-radius: 8px;

  position: sticky;
  top: 0;
  z-index: 50;
  flex: 0 0 auto;
  height: var(--vf-topbar-height);
  /* 填充面令牌：深色主题下仍保持足够的蓝（白字/白图标 5.45:1）。 */
  background: var(--vf-accent-fill);
}

/* 白色前景只作用于顶栏内的导航控件（右侧弹窗/下拉不继承，避免白底白字）。 */
.app-header-inner {
  display: flex;
  align-items: center;
  gap: var(--vf-space-2);
  height: 100%;
  min-width: 0;
  /* 与页面内容同一条 16px 左右基线 */
  padding: 0 var(--vf-page-padding);
  flex-wrap: nowrap;
  color: var(--vf-on-accent);
}

.app-header-left {
  display: flex;
  align-items: center;
  gap: 0.35rem;
  min-width: 0;
  flex: 0 1 auto;
}

.app-header-brand {
  display: inline-flex;
  align-items: center;
  gap: var(--vf-space-2);
  height: 100%;
  /* 16（容器）+ 4 = 20px，与侧栏首列图标对齐 */
  padding: 0 var(--vf-space-1);
  color: var(--vf-on-accent);
  text-decoration: none;
  white-space: nowrap;
}

.app-header-brand:hover,
.app-header-brand:focus-visible {
  color: var(--vf-on-accent);
  opacity: 0.92;
}

/* 品牌图标：保留原始彩虹渐变（白 V 自带对比度）。
   此前用 brightness(0) invert(1) 想染白，结果把整块不透明底也刷成白色，
   蓝底上只剩一个白色圆角方块（V 被吃掉）。 */
.app-header-logo {
  width: 28px;
  height: 28px;
  flex: 0 0 auto;
}

.app-header-brand-name {
  font-size: 0.95rem;
  font-weight: 650;
  letter-spacing: 0.01em;
  color: var(--vf-on-accent);
}

.app-header-apps {
  display: flex;
  align-items: center;
  gap: 0.15rem;
  margin-left: 0.35rem;
  flex: 0 0 auto;
}

/* 应用图标：40px 命中区，白色图标；当前应用用白色半透明圆角块表示选中。 */
.app-header-app {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 40px;
  height: 40px;
  border-radius: var(--vf-header-radius);
  color: var(--vf-on-accent);
  text-decoration: none;
  transition:
    background-color 0.12s var(--vf-motion-standard),
    color 0.12s var(--vf-motion-standard);
}

.app-header-app:hover,
.app-header-app:focus-visible {
  background: var(--vf-header-hover);
  color: var(--vf-on-accent);
}

.app-header-app:focus-visible {
  outline: 2px solid var(--vf-on-accent);
  outline-offset: -2px;
}

.app-header-app.is-active {
  background: var(--vf-header-active);
  color: var(--vf-on-accent);
}

.app-header-center {
  display: flex;
  align-items: center;
  min-width: 0;
  margin-left: 0.5rem;
}

.app-header-actions {
  display: flex;
  align-items: center;
  gap: 0.15rem;
  margin-left: auto;
  flex: 0 0 auto;
}

/* 右侧图标（搜索 / 通知 / 主题）在主色面上统一白色 + 半透明白 hover。 */
.app-header-actions :deep(.vf-icon-button) {
  color: var(--vf-on-accent);
}

.app-header-actions :deep(.vf-icon-button:hover:not(:disabled)),
.app-header-actions :deep(.vf-icon-button:focus-visible) {
  background: var(--vf-header-hover);
  color: var(--vf-on-accent);
}

.app-header-actions :deep(.notification-center .app-bar-bell.is-active) {
  background: var(--vf-header-active);
  color: var(--vf-on-accent);
}

/* 主题切换是 Bulma 的浅灰胶囊按钮：顶栏里改成无底白字，避免在白顶上“消失”。 */
.app-header-actions :deep(.button.is-light) {
  min-height: 34px;
  border-color: transparent;
  background: transparent;
  color: var(--vf-on-accent);
}

.app-header-actions :deep(.button.is-light:hover),
.app-header-actions :deep(.button.is-light:focus-visible) {
  background: var(--vf-header-hover);
  border-color: transparent;
  color: var(--vf-on-accent);
}

/* ---- 账号菜单（原 Home 顶栏账号下拉，迁入顶栏组件） ---- */
.app-header-account {
  display: inline-flex;
  align-items: center;
  gap: 0.45rem;
  height: 2.125rem;
  max-width: 12rem;
  padding: 0 0.55rem 0 0.25rem;
  border: 1px solid transparent;
  border-radius: var(--vf-header-radius);
  background: transparent;
  color: var(--vf-on-accent);
  font-size: 0.875rem;
  cursor: pointer;
  transition:
    background-color 0.15s var(--vf-motion-standard),
    border-color 0.15s var(--vf-motion-standard);
}

.app-header-account:hover,
.app-header-account:focus-visible {
  background: var(--vf-header-hover);
  border-color: transparent;
  color: var(--vf-on-accent);
}

.app-header-account:focus-visible {
  outline: 2px solid var(--vf-on-accent);
  outline-offset: -2px;
}

/* 头像：白色圆形底 + 深色首字母（蓝底上仍是清晰的账号标识）。 */
.app-header-avatar {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  flex: 0 0 auto;
  width: 1.5rem;
  height: 1.5rem;
  border-radius: 50%;
  background: var(--vf-on-accent);
  color: var(--vf-accent-text);
  font-size: 0.75rem;
  font-weight: 700;
}

.app-header-account-name {
  min-width: 0;
  overflow: hidden;
  white-space: nowrap;
  text-overflow: ellipsis;
  color: var(--vf-on-accent);
}

.app-header-account-caret {
  flex: 0 0 auto;
  color: var(--vf-on-accent);
  opacity: 0.8;
}

.app-header-account-menu .dropdown-menu {
  min-width: 13rem;
}

.app-header-account-menu .dropdown-content {
  max-width: calc(100vw - 1rem);
  /* 下拉内容回到常规面：白底 + 深色文字，不继承顶栏的白色。 */
  color: var(--vf-text);
}

.app-header-account-summary {
  cursor: default;
}

.app-header-account-caption {
  margin: 0 0 0.2rem;
  color: var(--vf-text-muted);
  font-size: 0.72rem;
}

.app-header-account-profile {
  margin: 0;
  color: var(--vf-text);
  font-size: 0.8rem;
  word-break: break-all;
}

/* 移动端：只留图标，账号名收起；整行不换行、不溢出。 */
@media screen and (max-width: 768px) {
  .app-header-inner {
    gap: 0.15rem;
    padding: 0 0.4rem;
  }

  .app-header-brand {
    padding: 0 0.2rem;
  }

  .app-header-brand-name {
    display: none;
  }

  .app-header-apps {
    margin-left: 0.1rem;
  }

  .app-header-app {
    width: 36px;
    height: 36px;
  }

  .app-header-actions {
    gap: 0;
  }

  .app-header-search {
    width: 34px;
    height: 34px;
  }

  .app-header-account {
    justify-content: center;
    width: 34px;
    height: 34px;
    padding: 0;
  }

  .app-header-account-name,
  .app-header-account-caret {
    display: none;
  }
}

/* 窄屏（如 320px）：进一步收小图标，保证应用图标 + 右侧操作不溢出、不换行。 */
@media screen and (max-width: 400px) {
  .app-header-app,
  .app-header :deep(.vf-icon-button) {
    width: 32px;
    height: 32px;
  }

  .app-header-account {
    width: 32px;
    height: 32px;
  }
}
</style>

<!--
  深色主题：白底头像上的首字母改用填充深蓝。
  `--vf-accent-text` 在深色下是给深色背景用的浅蓝，落在白色头像上仅 1.8:1；
  scoped 样式命中不了 <html data-theme>，故单独一个全局块（与 ViewsSidebar 同法）。
-->
<style>
html[data-theme="dark"] .app-header-avatar {
  color: var(--vf-accent-fill);
}

@media screen and (prefers-color-scheme: dark) {
  :root:not([data-theme="light"]) .app-header-avatar {
    color: var(--vf-accent-fill);
  }
}
</style>
