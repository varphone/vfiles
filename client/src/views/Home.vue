<template>
  <div class="home">
    <FilesLayout title="全部文件">
      <template #sidebar>
        <ViewsSidebar />
      </template>

      <div class="home-browser-shell">
        <FileBrowser
          :key="auth.activeWorkspace || 'default-workspace'"
          ref="browserRef"
          :history-loading="dirHistoryLoading"
          :history-error="Boolean(dirHistoryError)"
          :history-state-label="desktopHistoryStateLabel"
          :history-title="desktopHistoryTitle"
          :history-has-commit="Boolean(browseCommit)"
          @history-fast-back="desktopHistoryFastBack"
          @history-step-back="desktopHistoryStepBack"
          @history-step-forward="desktopHistoryStepForward"
          @history-fast-forward="desktopHistoryFastForward"
        />
      </div>
    </FilesLayout>

    <div class="mobile-bottom-bar is-hidden-desktop">
      <div class="container is-fluid">
        <div class="mobile-bottom-bar-inner">
          <div class="mobile-bottom-bar-bottom">
            <div
              ref="actionMenuRef"
              class="dropdown is-up"
              :class="{ 'is-active': actionMenuOpen }"
            >
              <div class="dropdown-trigger">
                <button
                  class="button is-light is-small mobile-bar-button"
                  aria-haspopup="true"
                  :aria-expanded="actionMenuOpen ? 'true' : 'false'"
                  title="操作菜单"
                  aria-label="操作菜单"
                  @click="actionMenuOpen = !actionMenuOpen"
                >
                  <IconMenu2 :size="18" />
                  <span class="mobile-bar-label">更多</span>
                </button>
              </div>
              <div class="dropdown-menu" role="menu">
                <div class="dropdown-content">
                  <a
                    class="dropdown-item"
                    :class="{ 'is-current': actionMode === 'nav' }"
                    href="#"
                    @click.prevent="setActionMode('nav')"
                    >导航</a
                  >
                  <a
                    class="dropdown-item"
                    :class="{ 'is-current': actionMode === 'history' }"
                    href="#"
                    @click.prevent="setActionMode('history')"
                    >历史</a
                  >
                  <a
                    v-if="auth.canWrite"
                    class="dropdown-item"
                    :class="{ 'is-current': actionMode === 'batch' }"
                    href="#"
                    @click.prevent="setActionMode('batch')"
                    >批量</a
                  >
                </div>
              </div>
            </div>

            <div class="mobile-action-panel">
              <!-- 导航模式 -->
              <div
                v-if="actionMode === 'nav'"
                class="buttons are-small mb-0 mobile-action-buttons"
              >
                <button
                  class="button is-light mobile-bar-button"
                  title="上一级"
                  aria-label="上一级"
                  @click="goBack"
                >
                  <IconArrowLeft :size="18" />
                  <span class="mobile-bar-label">上一级</span>
                </button>
                <button
                  class="button is-light mobile-bar-button"
                  title="根目录"
                  aria-label="根目录"
                  @click="goRoot"
                >
                  <IconHome :size="18" />
                  <span class="mobile-bar-label">首页</span>
                </button>
                <button
                  v-if="!auth.isRestricted || auth.canWrite"
                  class="button is-light mobile-bar-button"
                  @click="appStore.requestCreateDirectory()"
                  title="新建文件夹"
                  aria-label="新建文件夹"
                >
                  <IconFolderPlus :size="18" />
                  <span class="mobile-bar-label">新建</span>
                </button>
                <button
                  v-if="!auth.isRestricted || auth.canWrite"
                  class="button is-link mobile-bar-button"
                  @click="openUploader"
                  title="上传"
                  aria-label="上传"
                >
                  <IconUpload :size="18" />
                  <span class="mobile-bar-label">上传</span>
                </button>
                <button
                  class="button is-light mobile-bar-button"
                  @click="refresh"
                  title="刷新"
                  aria-label="刷新"
                >
                  <IconRefresh :size="18" />
                  <span class="mobile-bar-label">刷新</span>
                </button>
              </div>

              <!-- 历史模式 -->
              <div
                v-else-if="actionMode === 'history'"
                class="mobile-history-panel"
              >
                <div class="mobile-history-hash">
                  <code class="is-size-7">{{ historyHashShort }}</code>
                  <span
                    v-if="historyDateShort"
                    class="is-size-7 vf-text-subtle ml-2"
                    >{{ historyDateShort }}</span
                  >
                </div>
                <div class="buttons are-small mb-0 mobile-action-buttons">
                  <button
                    class="button is-light"
                    :disabled="dirHistoryLoading || !canPrev"
                    @click="historyPrev"
                    title="向前"
                    aria-label="向前"
                  >
                    <IconChevronLeft :size="18" />
                  </button>
                  <button
                    class="button is-light"
                    :disabled="dirHistoryLoading || !canNext"
                    @click="historyNext"
                    title="向后"
                    aria-label="向后"
                  >
                    <IconChevronRight :size="18" />
                  </button>
                  <button
                    class="button is-light"
                    :disabled="dirHistoryLoading || !canLast"
                    @click="historyLast"
                    title="跳到最后"
                    aria-label="跳到最后"
                  >
                    <IconChevronsRight :size="18" />
                  </button>
                </div>
              </div>

              <!-- 批量模式 -->
              <div v-else class="mobile-batch-panel">
                <div class="is-size-7 vf-text-muted mr-2 mobile-batch-count">
                  已选 {{ selectedCount }}
                </div>
                <div class="mobile-batch-actions">
                  <div class="buttons are-small mb-0 mobile-action-buttons">
                    <button
                      class="button is-light"
                      @click="toggleBatch"
                      title="进入/退出批量"
                    >
                      <IconChecklist :size="18" />
                    </button>
                    <button
                      class="button is-light"
                      :disabled="!isBatchMode"
                      @click="selectAll"
                      title="全选"
                    >
                      全选
                    </button>
                    <button
                      class="button is-light"
                      :disabled="!isBatchMode"
                      @click="clearSelection"
                      title="取消选择"
                    >
                      取消
                    </button>
                    <button
                      class="button is-info"
                      :disabled="selectedCount === 0"
                      @click="batchDownload"
                      title="批量下载"
                    >
                      下载
                    </button>
                  </div>

                  <div
                    ref="batchMenuRef"
                    class="dropdown is-up mobile-batch-more"
                    :class="{ 'is-active': batchMenuOpen }"
                  >
                    <div class="dropdown-trigger">
                      <button
                        class="button is-light is-small"
                        aria-haspopup="true"
                        :aria-expanded="batchMenuOpen ? 'true' : 'false'"
                        title="更多批量操作"
                        @click="batchMenuOpen = !batchMenuOpen"
                      >
                        <IconDotsVertical :size="18" />
                      </button>
                    </div>
                    <div class="dropdown-menu" role="menu">
                      <div class="dropdown-content">
                        <a
                          v-if="auth.canWrite"
                          class="dropdown-item"
                          :class="{ 'is-disabled': selectedCount === 0 }"
                          href="#"
                          @click.prevent="runBatchMenuAction('delete')"
                        >
                          删除
                        </a>
                        <a
                          v-if="auth.canWrite"
                          class="dropdown-item"
                          :class="{ 'is-disabled': selectedCount === 0 }"
                          href="#"
                          @click.prevent="runBatchMenuAction('move')"
                        >
                          移动
                        </a>
                        <a
                          v-if="auth.canWrite"
                          class="dropdown-item"
                          :class="{ 'is-disabled': selectedCount !== 1 }"
                          href="#"
                          @click.prevent="runBatchMenuAction('rename')"
                        >
                          重命名
                        </a>
                      </div>
                    </div>
                  </div>
                </div>
              </div>
            </div>
          </div>
        </div>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref, watch } from "vue";
import { storeToRefs } from "pinia";
import { useRoute } from "vue-router";
import {
  IconMenu2,
  IconChecklist,
  IconDotsVertical,
  IconArrowLeft,
  IconChevronLeft,
  IconChevronRight,
  IconChevronsRight,
  IconHome,
  IconUpload,
  IconRefresh,
  IconFolderPlus,
} from "@tabler/icons-vue";
import FileBrowser from "../components/file-browser/FileBrowser.vue";
import FilesLayout from "../layouts/FilesLayout.vue";
import ViewsSidebar from "../components/files/ViewsSidebar.vue";
import { useFilesStore } from "../stores/files.store";
import { useAuthStore } from "../stores/auth.store";
import { useAppStore } from "../stores/app.store";
import { useDirectoryHistory } from "../composables/useDirectoryHistory";
import { isPathWithinRoots, normalizeScopePath } from "../utils/sessionScope";

type FileBrowserHandle = {
  openUploader: () => void;
  refresh: () => void;
  goBack: () => void;
  goRoot: () => void;
  toggleBatchMode: () => void;
  setSearchQuery: (q: string) => void;
  runSearch: () => Promise<void>;
  clearSearch: () => void;
  /** defineExpose 暴露的 ref 经 proxyRefs 解包后即为值本身（不是 Ref）。 */
  batchMode: boolean;
  selectedCount: number;
  selectAllVisible: () => void;
  clearSelection: () => void;
  batchDownload: () => Promise<void>;
  batchDelete: () => Promise<void>;
  batchMove: () => Promise<void>;
  renameSelected: () => Promise<void>;
  searchLoading: boolean;
};

const browserRef = ref<FileBrowserHandle | null>(null);

const auth = useAuthStore();
const appStore = useAppStore();

const actionMenuOpen = ref(false);
const actionMenuRef = ref<HTMLElement | null>(null);
const actionMode = ref<"nav" | "history" | "batch">("nav");
const batchMenuOpen = ref(false);
const batchMenuRef = ref<HTMLElement | null>(null);
const filesStore = useFilesStore();
const { currentPath, browseCommit } = storeToRefs(filesStore);
const route = useRoute();

/**
 * 侧栏目录树用 `/?path=目录` 交付导航目标。Home 是唯一消费这个参数的页面：
 * 其它视图（最近/收藏/活动）没有 Home，切过去不受影响。
 */
let treeQueryApplied: string | null = null;

function applyTreeQueryPath(raw: unknown) {
  if (typeof raw !== "string") return;
  const target = normalizeScopePath(raw);
  // 受限会话不接受范围外路径（侧栏树本身也只用 scopeRoots 渲染）。
  if (!isPathWithinRoots(target, auth.scopeRoots)) return;
  treeQueryApplied = target;
  if (target === filesStore.requestedPath) return;
  filesStore.navigateTo(target);
}

watch(
  () => route.query.path,
  (value) => {
    // 路由切到别的视图时 Home 会卸载；这里再挡一层，避免离开首页的瞬间
    // 改动文件区状态。
    if (route.path !== "/") return;
    if (typeof value === "string") {
      applyTreeQueryPath(value);
      return;
    }
    // 从目录树目标回到不带参数的 `/`（例如点「全部文件」）→ 回根目录。
    if (treeQueryApplied !== null) {
      treeQueryApplied = null;
      if (filesStore.requestedPath !== "") filesStore.navigateTo("");
    }
  },
);

// 版本切换：加载/步进/取消都在 composable 内，Home 负责状态，FileBrowser 工具栏负责展示。
const {
  loading: dirHistoryLoading,
  error: dirHistoryError,
  commits: dirHistoryCommits,
  loaded: dirHistoryLoaded,
  selectedHash: dirSelectedHash,
  selectedIndex: dirSelectedIndex,
  canPrev,
  canNext,
  canLast,
  load: loadHistory,
  ensureLoaded: ensureHistoryLoaded,
  select: applyHistoryHash,
  stepBack: desktopHistoryStepBack,
  stepForward: desktopHistoryStepForward,
  fastBack: desktopHistoryFastBack,
  fastForward: desktopHistoryFastForward,
} = useDirectoryHistory({
  currentPath,
  browseCommit,
  setBrowseCommit: (commit) => filesStore.setBrowseCommit(commit),
});

function updateVisualViewportBottomOffset() {
  const vv = window.visualViewport;
  if (!vv) {
    document.documentElement.style.setProperty("--vv-bottom", "0px");
    return;
  }

  // iOS Safari 等浏览器的底部工具栏会压住 layout viewport 的 fixed 元素。
  // 用 visualViewport 的可视高度计算被遮挡的底部偏移，并通过 CSS 变量抬起底部操作栏。
  const bottom = Math.max(0, window.innerHeight - (vv.height + vv.offsetTop));
  document.documentElement.style.setProperty("--vv-bottom", `${bottom}px`);
}

onMounted(() => {
  // FileBrowser 先按 store 里上次的目录挂载；父组件的 mounted 晚于子组件，
  // 这里再用 URL 里的目录树目标覆盖它（直接打开或从其它视图跳回）。
  applyTreeQueryPath(route.query.path);
  updateVisualViewportBottomOffset();
  window.addEventListener("resize", updateVisualViewportBottomOffset);
  window.visualViewport?.addEventListener(
    "resize",
    updateVisualViewportBottomOffset,
  );
  window.visualViewport?.addEventListener(
    "scroll",
    updateVisualViewportBottomOffset,
  );

  const onDocPointer = (e: MouseEvent | TouchEvent) => {
    const target = e.target as Node | null;
    if (!target) return;

    if (actionMenuOpen.value) {
      const el = actionMenuRef.value;
      if (el && !el.contains(target)) actionMenuOpen.value = false;
    }
    if (batchMenuOpen.value) {
      const el = batchMenuRef.value;
      if (el && !el.contains(target)) batchMenuOpen.value = false;
    }
  };

  const onDocKeydown = (e: KeyboardEvent) => {
    if (e.key !== "Escape") return;
    actionMenuOpen.value = false;
    batchMenuOpen.value = false;
  };

  document.addEventListener("click", onDocPointer, true);
  document.addEventListener("touchstart", onDocPointer, true);
  document.addEventListener("keydown", onDocKeydown);

  onBeforeUnmount(() => {
    document.removeEventListener("click", onDocPointer, true);
    document.removeEventListener("touchstart", onDocPointer, true);
    document.removeEventListener("keydown", onDocKeydown);
  });
});

onBeforeUnmount(() => {
  window.removeEventListener("resize", updateVisualViewportBottomOffset);
  window.visualViewport?.removeEventListener(
    "resize",
    updateVisualViewportBottomOffset,
  );
  window.visualViewport?.removeEventListener(
    "scroll",
    updateVisualViewportBottomOffset,
  );
});

function openUploader() {
  browserRef.value?.openUploader();
}

function refresh() {
  browserRef.value?.refresh();
}

function goBack() {
  browserRef.value?.goBack();
}

function goRoot() {
  browserRef.value?.goRoot();
}

function toggleBatch() {
  browserRef.value?.toggleBatchMode();
}

const isBatchMode = computed(() => browserRef.value?.batchMode ?? false);
const selectedCount = computed(() => browserRef.value?.selectedCount ?? 0);
function setActionMode(mode: "nav" | "history" | "batch") {
  if (mode === "batch" && !auth.canWrite) return;
  actionMode.value = mode;
  actionMenuOpen.value = false;
  batchMenuOpen.value = false;

  // 进入批量模式时自动开启批量
  if (mode === "batch" && !isBatchMode.value) {
    toggleBatch();
  }

  // 离开历史模式时回到当前版本（HEAD/worktree）
  if (mode !== "history" && browseCommit.value) {
    filesStore.setBrowseCommit(undefined);
  }

  if (mode === "history") {
    void loadHistory();
  }
}

const historyHashShort = computed(() => {
  const h = dirSelectedHash.value || browseCommit.value || "";
  return h ? h.substring(0, 8) : "--------";
});

const historyDateShort = computed(() => {
  const idx = dirSelectedIndex.value;
  const iso = idx >= 0 ? dirHistoryCommits.value[idx]?.date : undefined;
  if (!iso) return "";
  const d = new Date(iso);
  if (Number.isNaN(d.getTime())) return "";

  // 紧凑显示：MM-DD HH:mm（使用本地时区）
  const mm = String(d.getMonth() + 1).padStart(2, "0");
  const dd = String(d.getDate()).padStart(2, "0");
  const hh = String(d.getHours()).padStart(2, "0");
  const mi = String(d.getMinutes()).padStart(2, "0");
  return `${mm}-${dd} ${hh}:${mi}`;
});

const desktopHistoryStateLabel = computed(() => {
  if (dirHistoryLoading.value) return "加载历史...";
  // 失败后按钮恢复可用，标签直接说明这次没拿到历史。
  if (dirHistoryError.value) return "加载失败";
  // 目录确实没有可回退的快照时也要说清楚，否则按钮点了像是没反应。
  if (dirHistoryLoaded.value && dirHistoryCommits.value.length === 0) {
    return "无历史";
  }
  if (browseCommit.value) return historyHashShort.value;
  return "当前版本";
});

/** 桌面工具栏的操作区：失败时把具体原因和重试方式放进悬停提示。 */
const desktopHistoryTitle = computed(() =>
  dirHistoryError.value
    ? `${dirHistoryError.value}（点击后退重试）`
    : desktopHistoryStateLabel.value,
);

/**
 * 手机端历史面板的按钮：在已加载的提交列表内移动；列表还没加载时先加载。
 */
async function historyPrev() {
  if (!(await ensureHistoryLoaded())) return;
  const idx = dirSelectedIndex.value;
  if (idx < 0) return;
  applyHistoryHash(dirHistoryCommits.value[idx + 1]?.hash);
}

async function historyNext() {
  if (!(await ensureHistoryLoaded())) return;
  const idx = dirSelectedIndex.value;
  if (idx <= 0) return;
  applyHistoryHash(dirHistoryCommits.value[idx - 1]?.hash);
}

async function historyLast() {
  if (!(await ensureHistoryLoaded())) return;
  const lastHash = dirHistoryCommits.value[0]?.hash;
  if (!lastHash) return;
  applyHistoryHash(lastHash);
}

async function runBatchMenuAction(action: "delete" | "move" | "rename") {
  if (action === "delete" && selectedCount.value === 0) return;
  if (action === "move" && selectedCount.value === 0) return;
  if (action === "rename" && selectedCount.value !== 1) return;

  batchMenuOpen.value = false;

  if (action === "delete") await batchDelete();
  if (action === "move") await batchMove();
  if (action === "rename") await renameSelected();
}

function selectAll() {
  browserRef.value?.selectAllVisible();
}

function clearSelection() {
  browserRef.value?.clearSelection();
}

async function batchDownload() {
  await browserRef.value?.batchDownload();
}

async function batchDelete() {
  await browserRef.value?.batchDelete();
}

async function batchMove() {
  await browserRef.value?.batchMove();
}

async function renameSelected() {
  await browserRef.value?.renameSelected();
}
</script>

<style scoped>
/* 页面背景与外壳同色：内容面板（FilesLayout 的 .files-content）是唯一焦点。 */
.home {
  position: relative;
  min-height: 100dvh;
  /* 兼容文件列表内部的高度计算（本阶段不改列表，下一阶段统一改用 --vf-topbar-height） */
  --sf-topbar-height: var(--vf-topbar-height);
  /* 用 clip 而不是 hidden：hidden 会让 .home 变成滚动容器，导致内部的
     position: sticky（批量操作条）失效；clip 只裁剪不建立滚动容器。
     旧浏览器不支持 clip 时退回 hidden（仅失去吸顶效果）。 */
  overflow-x: clip;
  background: var(--vf-shell-bg);
}

/* 文件列表自己铺满内容面板：内部 16px 单元格基线已由表格/工具栏持有。 */
.home :deep(.files-content) {
  padding: 0;
}

/* 文件区直接落在圆角面板上：去掉 FileBrowser 自己的卡片高度与底色差。 */
.home :deep(.file-browser-box) {
  height: 100%;
  background: var(--vf-content-bg);
}

.home-browser-shell {
  position: relative;
  display: flex;
  flex-direction: column;
  flex: 1 1 auto;
  min-width: 0;
  min-height: 0;
}

@supports not (overflow: clip) {
  .home {
    overflow-x: hidden;
  }
}

.mobile-bottom-bar {
  position: fixed;
  left: 0;
  right: 0;
  bottom: 0;
  z-index: var(--vf-z-bottombar);
  background: var(--vf-surface);
  border-top: 1px solid var(--vf-border-weak);
  box-shadow: none;
}

.mobile-bottom-bar-inner {
  width: 100%;
  display: flex;
  flex-direction: column;
  align-items: stretch;
  padding: 0.35rem 0.5rem calc(0.35rem + env(safe-area-inset-bottom));
}

/* 图标 + 文字：触屏没有 hover，底部操作必须自解释 */
.mobile-bar-button {
  display: inline-flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  gap: 0.1rem;
  min-height: 2.75rem;
  padding: 0.25rem 0.35rem;
  line-height: 1.1;
}

.mobile-bar-label {
  font-size: 0.66rem;
  font-weight: 500;
  letter-spacing: 0.01em;
}

.mobile-bottom-bar-bottom {
  display: flex;
  align-items: center;
  gap: 0.35rem;
}

.mobile-action-panel {
  flex: 1;
  min-width: 0;
  display: flex;
  align-items: center;
  justify-content: flex-end;
}

.mobile-batch-panel {
  width: 100%;
  min-width: 0;
  display: flex;
  align-items: center;
  justify-content: flex-end;
}

.mobile-history-panel {
  width: 100%;
  min-width: 0;
  display: flex;
  align-items: center;
  justify-content: flex-end;
  gap: 0.5rem;
}

.mobile-history-hash {
  flex: 0 0 auto;
}

.mobile-batch-actions {
  flex: 1;
  min-width: 0;
  display: flex;
  align-items: center;
  justify-content: flex-end;
  margin-left: auto;
  gap: 0.25rem;
  overflow: visible;
}

.mobile-batch-more {
  flex: 0 0 auto;
}

.mobile-batch-more .dropdown-menu {
  left: auto;
  right: 0;
  max-width: calc(100vw - 1rem);
}

.mobile-batch-count {
  flex: 0 0 auto;
}

/* 底部操作按钮：与桌面工具栏一致的图标按钮语言 */
/* 按钮等分剩余宽度且不换行：固定宽度会让 5 个按钮超出面板宽度并叠在一起 */
.mobile-action-buttons {
  display: flex;
  flex-wrap: nowrap;
  width: 100%;
  /* M3/HIG：相邻触控目标间距 ≥8px（原 2.4px 过紧，几乎相贴） */
  gap: 0.5rem;
}

.mobile-action-buttons .button {
  display: inline-flex;
  flex: 1 1 0;
  align-items: center;
  justify-content: center;
  width: auto;
  min-width: 0;
  height: 2.75rem;
  padding: 0 0.1rem;
  border: none;
  border-radius: var(--vf-radius-control);
  background: transparent;
  color: var(--vf-text-muted);
}

.mobile-action-buttons .button:hover,
.mobile-action-buttons .button:active {
  background: var(--vf-row-hover);
  color: var(--vf-text);
}

.mobile-action-buttons .button.is-link {
  background: var(--vf-accent-fill);
  color: var(--vf-on-accent);
}

.mobile-action-buttons .button.is-link:hover {
  background: var(--vf-accent-fill-hover);
  color: var(--vf-on-accent);
}

.mobile-action-buttons {
  flex: 1;
  min-width: 0;
  overflow-x: auto;
  flex-wrap: nowrap;
  white-space: nowrap;
  justify-content: flex-end;
  margin-left: auto;
}

.dropdown-item.is-disabled {
  opacity: 0.45;
  pointer-events: none;
}

@media screen and (min-width: 1024px) {
  /* 桌面端文件列表在自己的内容区内滚动：工具栏、列头、状态栏与详情面板始终可见。 */
  .home :deep(.files-content) {
    overflow: hidden;
  }

  .mobile-bottom-bar {
    display: none;
  }

  :deep(.file-browser-box) {
    display: flex;
    flex-direction: column;
  }

}

@media screen and (max-width: 1023px) {
  .mobile-bottom-bar {
    /* 安全区只由 .mobile-bottom-bar-inner 的 padding 承担一次；
       这里再抬 env() 会让底栏悬浮、下方露出页面底色 */
    bottom: var(--vv-bottom, 0px);
  }

  /* 为 fixed 底部操作栏留出空间，并在 iOS 浏览器工具栏遮挡时一起抬起。 */
  .home-browser-shell {
    min-height: calc(100vh - var(--vf-topbar-height));
    min-height: calc(100dvh - var(--vf-topbar-height));
    padding-bottom: calc(6rem + var(--vv-bottom, 0px));
  }
}
</style>
