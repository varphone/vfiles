<template>
  <nav class="views-sidebar" aria-label="视图">
    <!--
      侧栏搜索：本阶段只做占位与视觉，回车把关键字 emit 给页面；
      后续接入统一搜索（AppHeader 的 UnifiedSearch / useFileSearch）后再补过滤逻辑。
    -->
    <div class="views-sidebar-search">
      <IconSearch
        class="views-sidebar-search-icon"
        :size="16"
        aria-hidden="true"
      />
      <input
        v-model="searchDraft"
        class="views-sidebar-search-input"
        type="search"
        placeholder="Search here …"
        aria-label="搜索文件"
        @keydown.enter.prevent="submitSearch"
      />
      <button
        v-if="searchDraft"
        class="views-sidebar-search-clear"
        type="button"
        aria-label="清空搜索"
        @click="clearSearch"
      >
        <IconX :size="14" aria-hidden="true" />
      </button>
    </div>

    <!--
      分组标题而不是可点击条目：Nextcloud 的「Personal files」只是视觉分组，
      「全部文件」与它同路由；做成标题就不会出现两个条目同时 aria-current。
    -->
    <h2 id="views-sidebar-personal" class="views-sidebar-group">个人文件</h2>

    <ul class="views-sidebar-list" aria-labelledby="views-sidebar-personal">
      <li v-for="item in items" :key="item.key">
        <RouterLink
          class="views-sidebar-item"
          :class="{ 'is-active': isActive(item.to) }"
          :to="item.to"
          :aria-current="isActive(item.to) ? 'page' : undefined"
        >
          <component :is="item.icon" :size="18" :stroke-width="1.7" />
          <span class="views-sidebar-label">{{ item.label }}</span>
        </RouterLink>
      </li>
    </ul>

    <!--
      可折叠的目录树（Nextcloud 的 Folder tree）：默认折叠，展开后占满
      侧栏剩余高度并内部滚动，底部存储/设置/已删除三项不被挤压。
    -->
    <div class="views-sidebar-tree" :class="{ 'is-expanded': folderTreeOpen }">
      <button
        id="views-sidebar-tree-toggle"
        class="views-sidebar-tree-toggle"
        type="button"
        aria-controls="views-sidebar-tree-panel"
        :aria-expanded="folderTreeOpen ? 'true' : 'false'"
        @click="folderTreeOpen = !folderTreeOpen"
      >
        <span class="views-sidebar-tree-label">文件夹树</span>
        <IconChevronDown
          v-if="folderTreeOpen"
          class="views-sidebar-tree-chevron"
          :size="16"
          aria-hidden="true"
        />
        <IconChevronRight
          v-else
          class="views-sidebar-tree-chevron"
          :size="16"
          aria-hidden="true"
        />
      </button>

      <div
        v-if="folderTreeOpen"
        id="views-sidebar-tree-panel"
        class="views-sidebar-tree-body"
      >
        <DirectoryTree
          :current-path="currentPath"
          :roots="auth.scopeRoots"
          :can-write="auth.canWrite"
          @navigate="navigateFromTree"
        />
      </div>
    </div>

    <!-- 底部固定区：存储用量 / 文件设置 / 已删除（最后一项暂不支持）。 -->
    <ul class="views-sidebar-footer">
      <!-- 存储用量：受限会话看不到工作区统计，整项隐藏。 -->
      <li v-if="showStorage" class="views-sidebar-footer-item">
        <div class="views-sidebar-item views-sidebar-usage" aria-label="存储用量">
          <IconChartPie :size="18" :stroke-width="1.7" aria-hidden="true" />
          <span class="views-sidebar-label">存储用量</span>
          <span v-if="storage" class="views-sidebar-usage-value">
            {{ formatSize(storage.total_bytes) }}
          </span>
          <span v-else class="views-sidebar-usage-value is-muted">
            {{ storageError ? "暂不可用" : "加载中…" }}
          </span>
        </div>
        <p v-if="storage" class="views-sidebar-usage-meta">
          {{ storage.file_count }} 文件 · {{ storage.directory_count }} 目录
        </p>
      </li>

      <li class="views-sidebar-footer-item">
        <RouterLink class="views-sidebar-item" to="/settings/tokens">
          <IconSettings :size="18" :stroke-width="1.7" aria-hidden="true" />
          <span class="views-sidebar-label">文件设置</span>
        </RouterLink>
      </li>

      <li class="views-sidebar-footer-item">
        <!-- 回收站能力尚未接入：保留入口位置但明确禁用，避免点了没反应。 -->
        <button
          class="views-sidebar-item views-sidebar-item--disabled"
          type="button"
          title="暂不支持"
          aria-disabled="true"
          disabled
        >
          <IconTrash :size="18" :stroke-width="1.7" aria-hidden="true" />
          <span class="views-sidebar-label">已删除</span>
        </button>
      </li>
    </ul>
  </nav>
</template>

<script setup lang="ts">
import { computed, onMounted, ref, type Component } from "vue";
import { storeToRefs } from "pinia";
import { useRoute, useRouter } from "vue-router";
import {
  IconActivity,
  IconChartPie,
  IconChevronDown,
  IconChevronRight,
  IconClock,
  IconFolder,
  IconSearch,
  IconSettings,
  IconShare,
  IconStar,
  IconTrash,
  IconX,
} from "@tabler/icons-vue";
import DirectoryTree from "../file-browser/DirectoryTree.vue";
import { filesService } from "../../services/files.service";
import { useAuthStore } from "../../stores/auth.store";
import { useFilesStore } from "../../stores/files.store";
import { formatSize } from "../../utils/filePresentation";
import { normalizeScopePath } from "../../utils/sessionScope";
import type { WorkspaceOverview } from "../../types";

/**
 * Nextcloud 式视图侧栏：「个人文件」分组标题下是全部文件 / 最近 / 收藏 /
 * 分享 / 活动，再往下是可折叠的文件夹树，底部是存储用量、文件设置与
 * （暂不支持的）已删除。
 *
 * 每行 44px，选中项为主色实心蓝底 + 白字 + 8px 圆角；底部区域用
 * `margin-top: auto` 贴住侧栏底部。搜索框本阶段只做占位，回车只上报事件。
 */
type ViewItem = {
  key: string;
  to: string;
  label: string;
  icon: Component;
};

const emit = defineEmits<{
  /** 侧栏搜索占位：回车把关键字交给页面（后续接统一搜索）。 */
  (e: "search", query: string): void;
}>();

const auth = useAuthStore();
const filesStore = useFilesStore();
const { currentPath } = storeToRefs(filesStore);
const route = useRoute();
const router = useRouter();

const storage = ref<WorkspaceOverview | null>(null);
const storageError = ref(false);
/** 侧栏搜索框的当前输入（本阶段不参与过滤）。 */
const searchDraft = ref("");
/** 文件夹树默认折叠：首次展开时才挂载 DirectoryTree（按需请求根目录）。 */
const folderTreeOpen = ref(false);

/** 受限会话不展示工作区统计（与账号级入口同策略）。 */
const showStorage = computed(() => !auth.isRestricted);

const items = computed<ViewItem[]>(() => {
  const entries: ViewItem[] = [
    { key: "all", to: "/", label: "全部文件", icon: IconFolder },
    { key: "recent", to: "/recent", label: "最近", icon: IconClock },
    { key: "favorites", to: "/favorites", label: "收藏", icon: IconStar },
  ];

  // 分享视图依赖分享功能且不适用于受限会话（路由守卫同样会回首页）。
  if (!auth.isRestricted && auth.features?.shareEnabled !== false) {
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

/** 「全部文件」只在根视图高亮，其余按前缀匹配子路径。 */
function isActive(to: string): boolean {
  if (to === "/") return route.path === "/";
  return route.path === to || route.path.startsWith(`${to}/`);
}

/**
 * 目录树点击：用 `/?path=目录` 表达目标，由 Home 消费并把文件区导航过去。
 * 路由完全不变时（已在首页且 query 相同）router.push 不会触发 watch，
 * 这里直接让文件区回到该目录，避免“点了没反应”。
 */
function navigateFromTree(path: string) {
  const target = normalizeScopePath(path);
  const currentQuery = normalizeScopePath(
    typeof route.query.path === "string" ? route.query.path : "",
  );
  if (route.path === "/" && target === currentQuery) {
    filesStore.navigateTo(target);
    return;
  }
  void router.push({ path: "/", query: { path: target } });
}

/** 回车提交：先 emit，过滤逻辑留给后续的统一搜索接入。 */
function submitSearch() {
  const query = searchDraft.value.trim();
  if (!query) return;
  emit("search", query);
}

function clearSearch() {
  searchDraft.value = "";
}

async function loadOverview() {
  if (!showStorage.value) return;
  storageError.value = false;
  try {
    storage.value = await filesService.getOverview();
  } catch {
    storageError.value = true;
  }
}

onMounted(() => {
  void loadOverview();
});
</script>

<style scoped>
/*
 * 侧栏本体：背景渐变由 FilesLayout 的 .files-sidebar 承担（同 300px 列），
 * 这里只排内容。令牌全部用 --vf-*，深色主题各一套。
 */
.views-sidebar {
  --vf-sidebar-item-radius: var(--vf-radius-card);
  --vf-sidebar-item-hover: rgba(0, 0, 0, 0.04);
  --vf-sidebar-item-active-bg: var(--vf-accent);
  --vf-sidebar-item-active-fg: var(--vf-on-accent);
  --vf-sidebar-search-bg: #ffffff;

  display: flex;
  flex-direction: column;
  height: 100%;
  min-height: 0;
  /* 与条目同宽：搜索框、导航、底部区共用 8px 外边距 */
  padding: var(--vf-space-2);
}

/* ---- 顶部搜索（占位） ---- */
.views-sidebar-search {
  position: relative;
  display: flex;
  align-items: center;
  margin: 0 0 var(--vf-space-2);
  flex: 0 0 auto;
}

.views-sidebar-search-icon {
  position: absolute;
  left: var(--vf-space-3);
  color: var(--vf-text-muted);
  pointer-events: none;
}

.views-sidebar-search-input {
  width: 100%;
  height: var(--vf-control-height);
  /* 图标 12px 起、文字 36px 起；右侧给清空按钮 32px */
  padding: 0 var(--vf-space-6) 0 36px;
  border: 1px solid var(--vf-border-weak);
  border-radius: var(--vf-radius-control);
  background: var(--vf-sidebar-search-bg);
  color: var(--vf-text);
  font-size: 0.85rem;
  appearance: none;
}

.views-sidebar-search-input::placeholder {
  color: var(--vf-text-subtle);
}

/* type=search 的原生清空叉不要叠加到自定义 × 上。 */
.views-sidebar-search-input::-webkit-search-cancel-button {
  display: none;
}

.views-sidebar-search-input:focus {
  outline: none;
  border-color: var(--vf-accent);
  box-shadow: 0 0 0 3px var(--vf-focus-ring);
}

/* 清空按钮：仅在输入后出现，不占位时也保持 24px 命中区。 */
.views-sidebar-search-clear {
  position: absolute;
  right: var(--vf-space-2);
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 24px;
  height: 24px;
  padding: 0;
  border: none;
  border-radius: 50%;
  background: transparent;
  color: var(--vf-text-muted);
  cursor: pointer;
}

.views-sidebar-search-clear:hover,
.views-sidebar-search-clear:focus-visible {
  background: var(--vf-sidebar-item-hover);
  color: var(--vf-text-strong);
}

/* ---- 分组标题 ---- */
/*
 * 「个人文件」只是视觉分组（与 Nextcloud 的 Personal files 一致）：
 * 13px 灰色小标题，不是链接也不接收点击。上下 8px，落在间距基线上。
 */
.views-sidebar-group {
  margin: var(--vf-space-2) 0;
  padding: 0 var(--vf-space-3);
  color: var(--vf-text-muted);
  font-size: 13px;
  font-weight: 600;
  letter-spacing: 0.01em;
  line-height: 1.5;
}

/* ---- 视图条目 ---- */
.views-sidebar-list,
.views-sidebar-footer {
  display: flex;
  flex-direction: column;
  gap: var(--vf-space-1);
  margin: 0;
  padding: 0;
  list-style: none;
}

.views-sidebar-list {
  flex: 0 0 auto;
}

.views-sidebar-item {
  display: flex;
  align-items: center;
  gap: var(--vf-space-2);
  width: 100%;
  height: var(--vf-sidebar-item-height);
  padding: 0 var(--vf-space-3);
  border: none;
  border-radius: var(--vf-sidebar-item-radius);
  background: transparent;
  color: var(--vf-text);
  font-family: inherit;
  font-size: 0.875rem;
  font-weight: 500;
  text-align: left;
  text-decoration: none;
  white-space: nowrap;
  cursor: pointer;
}

.views-sidebar-item:hover,
.views-sidebar-item:focus-visible {
  background: var(--vf-sidebar-item-hover);
  color: var(--vf-text-strong);
}

.views-sidebar-item:focus-visible {
  outline: 2px solid var(--vf-accent);
  outline-offset: -2px;
}

/* 选中项：主色实心蓝底 + 白字 + 8px 圆角（不是浅蓝底蓝字）。 */
.views-sidebar-item.is-active {
  background: var(--vf-sidebar-item-active-bg);
  color: var(--vf-sidebar-item-active-fg);
  font-weight: 600;
}

.views-sidebar-item.is-active:hover,
.views-sidebar-item.is-active:focus-visible {
  background: var(--vf-sidebar-item-active-bg);
  color: var(--vf-sidebar-item-active-fg);
}

.views-sidebar-label {
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
}

/* ---- 文件夹树（可折叠分组） ---- */
.views-sidebar-tree {
  display: flex;
  flex-direction: column;
  flex: 0 0 auto;
  min-height: 0;
  margin-top: var(--vf-space-2);
}

/* 展开后吃掉视图列表与底部区之间的剩余高度（不改变底部三项的位置）。 */
.views-sidebar-tree.is-expanded {
  flex: 1 1 auto;
}

/*
 * 分组标题行：与「个人文件」同字号/同灰度，高度仍取 44px 条目基线；
 * 右侧箭头表明可展开/收起。折叠时整组只有这一行。
 */
.views-sidebar-tree-toggle {
  display: flex;
  align-items: center;
  gap: var(--vf-space-2);
  flex: 0 0 auto;
  width: 100%;
  height: var(--vf-sidebar-item-height);
  padding: 0 var(--vf-space-3);
  border: none;
  border-radius: var(--vf-sidebar-item-radius);
  background: transparent;
  color: var(--vf-text-muted);
  font-family: inherit;
  font-size: 13px;
  font-weight: 600;
  letter-spacing: 0.01em;
  text-align: left;
  cursor: pointer;
}

.views-sidebar-tree-toggle:hover,
.views-sidebar-tree-toggle:focus-visible {
  background: var(--vf-sidebar-item-hover);
  color: var(--vf-text-strong);
}

.views-sidebar-tree-toggle:focus-visible {
  outline: 2px solid var(--vf-accent);
  outline-offset: -2px;
}

.views-sidebar-tree-label {
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
}

.views-sidebar-tree-chevron {
  flex: 0 0 auto;
  margin-left: auto;
}

/*
 * 展开区：占满分组剩余高度并裁剪，滚动交给 DirectoryTree 自己的
 * `.directory-tree-scroll`（复用组件内部已有滚动容器）。
 */
.views-sidebar-tree-body {
  display: flex;
  flex: 1 1 auto;
  min-height: 0;
  margin-top: var(--vf-space-1);
  overflow: hidden;
}

/* 组件原本用于 FileBrowser 侧栏：去掉右分隔线与白底，融进侧栏渐变。 */
.views-sidebar-tree-body :deep(.directory-tree) {
  flex: 1 1 auto;
  min-height: 0;
  border-right: none;
  background: transparent;
}

.views-sidebar-tree-body :deep(.directory-tree-scroll) {
  padding: 0;
}

/* ---- 底部固定区 ---- */
.views-sidebar-footer {
  margin-top: auto;
  padding-top: var(--vf-space-2);
  border-top: 1px solid var(--vf-border-weak);
}

.views-sidebar-footer-item {
  min-width: 0;
}

/* 存储用量：不可点击的展示行（图标 + 名称 + 已用大小），下面补一行明细。 */
.views-sidebar-usage {
  cursor: default;
}

.views-sidebar-usage:hover {
  background: transparent;
  color: var(--vf-text);
}

.views-sidebar-usage-value {
  margin-left: auto;
  color: var(--vf-text-strong);
  font-size: 0.8125rem;
  font-weight: 600;
  white-space: nowrap;
}

.views-sidebar-usage-value.is-muted {
  color: var(--vf-text-muted);
  font-weight: 500;
}

.views-sidebar-usage-meta {
  margin: var(--vf-space-1) 0 0;
  padding: 0 var(--vf-space-3);
  color: var(--vf-text-muted);
  font-size: 0.75rem;
}

/* 已删除：禁用态（保留入口位置，不跳转）。 */
.views-sidebar-item--disabled {
  color: var(--vf-text-subtle);
  cursor: default;
  opacity: 0.55;
}

.views-sidebar-item--disabled:hover {
  background: transparent;
  color: var(--vf-text-subtle);
}
</style>

<!--
  深色主题的侧栏状态色：需要命中组件之外的祖先（<html data-theme> / 系统偏好），
  scoped 样式做不到，因此单独一个全局样式块；选择器带 html/:root 提升特异性，
  覆盖上面 scoped 块里的浅色取值。
-->
<style>
html[data-theme="dark"] .views-sidebar {
  --vf-sidebar-item-hover: rgba(255, 255, 255, 0.06);
  --vf-sidebar-item-active-bg: #0a6fa8;
  --vf-sidebar-search-bg: var(--vf-surface-sunken);
}

@media screen and (prefers-color-scheme: dark) {
  :root:not([data-theme="light"]) .views-sidebar {
    --vf-sidebar-item-hover: rgba(255, 255, 255, 0.06);
    --vf-sidebar-item-active-bg: #0a6fa8;
    --vf-sidebar-search-bg: var(--vf-surface-sunken);
  }
}
</style>
