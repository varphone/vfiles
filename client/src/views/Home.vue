<template>
  <div class="home">
    <!-- 移动端导航菜单 -->
    <nav class="navbar is-primary is-hidden-tablet is-fixed-top mobile-top-bar" role="navigation" aria-label="移动端导航">
      <div class="navbar-brand">
        <a class="navbar-item" href="#" @click.prevent="goRootAndClose">
          <IconFiles :size="22" class="mr-2" />
          <span>VFiles</span>
        </a>

        <a
          role="button"
          class="navbar-burger"
          :class="{ 'is-active': mobileMenuOpen }"
          aria-label="menu"
          :aria-expanded="mobileMenuOpen ? 'true' : 'false'"
          @click="mobileMenuOpen = !mobileMenuOpen"
        >
          <span aria-hidden="true"></span>
          <span aria-hidden="true"></span>
          <span aria-hidden="true"></span>
        </a>
      </div>

      <div class="navbar-menu" :class="{ 'is-active': mobileMenuOpen }">
        <div class="navbar-start">
          <a class="navbar-item" href="#" @click.prevent="goRootAndClose">根目录</a>
          <a class="navbar-item" href="#" @click.prevent="goBackAndClose">上一级</a>
          <a class="navbar-item" href="#" @click.prevent="refreshAndClose">刷新</a>
          <a class="navbar-item" href="#" @click.prevent="openUploaderAndClose">上传文件</a>
          <a class="navbar-item" href="#" @click.prevent="toggleBatchAndClose">批量模式</a>
        </div>

        <div class="navbar-end">
          <div class="navbar-item is-size-7 has-text-white-ter">
            当前：{{ currentPathLabel }}
          </div>
        </div>
      </div>
    </nav>

    <!-- 桌面端头部 -->
    <section class="hero is-primary is-small is-hidden-mobile">
      <div class="hero-body">
        <div class="container">
          <h1 class="title">
            <IconFiles :size="32" class="mr-2" />
            VFiles
          </h1>
          <p class="subtitle">基于Git的文件管理系统</p>
        </div>
      </div>
    </section>

    <section class="section home-content">
      <div class="container home-container">
        <FileBrowser ref="browserRef" />
      </div>
    </section>

    <!-- 移动端底部操作栏：左侧操作菜单 + 右侧动态操作区 -->
    <nav
      class="navbar is-fixed-bottom is-hidden-tablet has-background-dark mobile-bottom-bar"
      role="navigation"
      aria-label="底部操作栏"
    >
      <div class="mobile-bottom-bar-inner">
        <div class="mobile-bottom-bar-top">
          <!-- 搜索栏：始终显示（操作栏上半部分） -->
          <div class="field has-addons mb-0 mobile-search-field">
            <div class="control is-expanded">
              <input
                v-model="mobileSearchQuery"
                class="input is-small"
                type="text"
                placeholder="搜索..."
                @keyup.enter="runMobileSearch"
              />
            </div>
            <div class="control">
              <button
                class="button is-link is-small"
                :class="{ 'is-loading': isSearchLoading }"
                :disabled="isSearchLoading"
                @click="runMobileSearch"
              >
                <IconSearch :size="18" />
              </button>
            </div>
            <div class="control">
              <button class="button is-light is-small" :disabled="isSearchLoading" @click="clearMobileSearch">
                清空
              </button>
            </div>
          </div>
        </div>

        <div class="mobile-bottom-bar-bottom">
        <div class="dropdown is-up" :class="{ 'is-active': actionMenuOpen }">
          <div class="dropdown-trigger">
            <button
              class="button is-light is-small"
              aria-haspopup="true"
              :aria-expanded="actionMenuOpen ? 'true' : 'false'"
              @click="actionMenuOpen = !actionMenuOpen"
              title="操作菜单"
            >
              <IconMenu2 :size="18" />
            </button>
          </div>
          <div class="dropdown-menu" role="menu">
            <div class="dropdown-content">
              <a class="dropdown-item" href="#" @click.prevent="setActionMode('nav')">导航</a>
              <a class="dropdown-item" href="#" @click.prevent="setActionMode('history')">历史</a>
              <a class="dropdown-item" href="#" @click.prevent="setActionMode('batch')">批量</a>
            </div>
          </div>
        </div>

        <div class="mobile-action-panel">
          <!-- 导航模式 -->
          <div v-if="actionMode === 'nav'" class="buttons are-small mb-0 mobile-action-buttons">
            <button class="button is-light" @click="goBack" title="上一级">
              <IconArrowLeft :size="18" />
            </button>
            <button class="button is-light" @click="goRoot" title="根目录">
              <IconHome :size="18" />
            </button>
            <button class="button is-light" @click="openUploader" title="上传">
              <IconUpload :size="18" />
            </button>
            <button class="button is-light" @click="refresh" title="刷新">
              <IconRefresh :size="18" />
            </button>
          </div>

          <!-- 历史模式 -->
          <div v-else-if="actionMode === 'history'" class="mobile-history-panel">
            <div class="mobile-history-hash">
              <code class="is-size-7">{{ historyHashShort }}</code>
              <span v-if="historyDateShort" class="is-size-7 has-text-grey-light ml-2">{{ historyDateShort }}</span>
            </div>
            <div class="buttons are-small mb-0 mobile-action-buttons">
              <button class="button is-light" :disabled="dirHistoryLoading || !canPrev" @click="historyPrev" title="向前">
                向前
              </button>
              <button class="button is-light" :disabled="dirHistoryLoading || !canNext" @click="historyNext" title="向后">
                向后
              </button>
            </div>
          </div>

          <!-- 批量模式 -->
          <div v-else class="mobile-batch-panel">
            <div class="is-size-7 has-text-grey mr-2 mobile-batch-count">
              已选 {{ selectedCount }}
            </div>
            <div class="mobile-batch-actions">
              <div class="buttons are-small mb-0 mobile-action-buttons">
                <button class="button is-light" @click="toggleBatch" title="进入/退出批量">
                  <IconChecklist :size="18" />
                </button>
                <button class="button is-light" :disabled="!isBatchMode" @click="selectAll" title="全选">
                  全选
                </button>
                <button class="button is-light" :disabled="!isBatchMode" @click="clearSelection" title="取消选择">
                  取消
                </button>
                <button class="button is-info" :disabled="selectedCount === 0" @click="batchDownload" title="批量下载">
                  下载
                </button>
              </div>

              <div class="dropdown is-up mobile-batch-more" :class="{ 'is-active': batchMenuOpen }">
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
                      class="dropdown-item"
                      :class="{ 'is-disabled': selectedCount === 0 }"
                      href="#"
                      @click.prevent="runBatchMenuAction('delete')"
                    >
                      删除
                    </a>
                    <a
                      class="dropdown-item"
                      :class="{ 'is-disabled': selectedCount === 0 }"
                      href="#"
                      @click.prevent="runBatchMenuAction('move')"
                    >
                      移动
                    </a>
                    <a
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
    </nav>
  </div>
</template>

<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref } from 'vue';
import { storeToRefs } from 'pinia';
import {
  IconFiles,
  IconMenu2,
  IconSearch,
  IconChecklist,
  IconDotsVertical,
  IconArrowLeft,
  IconHome,
  IconUpload,
  IconRefresh,
} from '@tabler/icons-vue';
import FileBrowser from '../components/file-browser/FileBrowser.vue';
import { useFilesStore } from '../stores/files.store';
import { filesService } from '../services/files.service';

type FileBrowserHandle = {
  openUploader: () => void;
  refresh: () => void;
  goBack: () => void;
  goRoot: () => void;
  toggleBatchMode: () => void;
  setSearchQuery: (q: string) => void;
  runSearch: () => Promise<void>;
  clearSearch: () => void;
  batchMode: { value: boolean };
  selectedCount: { value: number };
  selectAllVisible: () => void;
  clearSelection: () => void;
  batchDownload: () => Promise<void>;
  batchDelete: () => Promise<void>;
  batchMove: () => Promise<void>;
  renameSelected: () => Promise<void>;
  searchLoading: { value: boolean };
};

const browserRef = ref<FileBrowserHandle | null>(null);
const mobileMenuOpen = ref(false);

const actionMenuOpen = ref(false);
const actionMode = ref<'nav' | 'history' | 'batch'>('nav');
const mobileSearchQuery = ref('');
const batchMenuOpen = ref(false);
const dirHistoryLoading = ref(false);
const dirHistoryError = ref<string | null>(null);
const dirHistoryCommits = ref<Array<{ hash: string; date?: string }>>([]);
const dirSelectedHash = ref('');
let dirHistoryReqId = 0;

const filesStore = useFilesStore();
const { currentPath, browseCommit } = storeToRefs(filesStore);

const currentPathLabel = computed(() => {
  return currentPath.value ? `/${currentPath.value}` : '根目录';
});

function updateVisualViewportBottomOffset() {
  const vv = window.visualViewport;
  if (!vv) {
    document.documentElement.style.setProperty('--vv-bottom', '0px');
    return;
  }

  // iOS Safari 等浏览器的底部工具栏会压住 layout viewport 的 fixed 元素。
  // 用 visualViewport 的可视高度计算被遮挡的底部偏移，并通过 CSS 变量抬起底部操作栏。
  const bottom = Math.max(0, window.innerHeight - (vv.height + vv.offsetTop));
  document.documentElement.style.setProperty('--vv-bottom', `${bottom}px`);
}

onMounted(() => {
  updateVisualViewportBottomOffset();
  window.addEventListener('resize', updateVisualViewportBottomOffset);
  window.visualViewport?.addEventListener('resize', updateVisualViewportBottomOffset);
  window.visualViewport?.addEventListener('scroll', updateVisualViewportBottomOffset);
});

onBeforeUnmount(() => {
  window.removeEventListener('resize', updateVisualViewportBottomOffset);
  window.visualViewport?.removeEventListener('resize', updateVisualViewportBottomOffset);
  window.visualViewport?.removeEventListener('scroll', updateVisualViewportBottomOffset);
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

const isBatchMode = computed(() => browserRef.value?.batchMode?.value ?? false);
const selectedCount = computed(() => browserRef.value?.selectedCount?.value ?? 0);
const isSearchLoading = computed(() => browserRef.value?.searchLoading?.value ?? false);

function setActionMode(mode: 'nav' | 'history' | 'batch') {
  actionMode.value = mode;
  actionMenuOpen.value = false;
  batchMenuOpen.value = false;

  // 进入批量模式时自动开启批量
  if (mode === 'batch' && !isBatchMode.value) {
    toggleBatch();
  }

  // 离开历史模式时回到当前版本（HEAD/worktree）
  if (mode !== 'history' && browseCommit.value) {
    filesStore.setBrowseCommit(undefined);
  }

  if (mode === 'history') {
    void loadDirHistory();
  }
}

const historyHashShort = computed(() => {
  const h = dirSelectedHash.value || browseCommit.value || '';
  return h ? h.substring(0, 8) : '--------';
});

const historyDateShort = computed(() => {
  const idx = dirSelectedIndex.value;
  const iso = idx >= 0 ? dirHistoryCommits.value[idx]?.date : undefined;
  if (!iso) return '';
  const d = new Date(iso);
  if (Number.isNaN(d.getTime())) return '';

  // 紧凑显示：MM-DD HH:mm（使用本地时区）
  const mm = String(d.getMonth() + 1).padStart(2, '0');
  const dd = String(d.getDate()).padStart(2, '0');
  const hh = String(d.getHours()).padStart(2, '0');
  const mi = String(d.getMinutes()).padStart(2, '0');
  return `${mm}-${dd} ${hh}:${mi}`;
});

const dirSelectedIndex = computed(() => {
  const h = dirSelectedHash.value;
  if (!h) return -1;
  return dirHistoryCommits.value.findIndex((c) => c.hash === h);
});

const canPrev = computed(() => {
  const idx = dirSelectedIndex.value;
  return idx >= 0 && idx < dirHistoryCommits.value.length - 1;
});

const canNext = computed(() => {
  const idx = dirSelectedIndex.value;
  return idx > 0;
});

async function loadDirHistory() {
  const reqId = ++dirHistoryReqId;
  dirHistoryLoading.value = true;
  dirHistoryError.value = null;
  try {
    const data = await filesService.getFileHistory(currentPath.value || '', 50);
    if (reqId !== dirHistoryReqId) return;
    dirHistoryCommits.value = (data.commits || []).map((c) => ({ hash: c.hash, date: c.date }));

    const preferred = browseCommit.value || dirSelectedHash.value || data.currentVersion || data.commits?.[0]?.hash || '';
    const exists = preferred && dirHistoryCommits.value.some((c) => c.hash === preferred);
    dirSelectedHash.value = exists ? preferred : (data.currentVersion || data.commits?.[0]?.hash || '');

    if (dirSelectedHash.value) {
      filesStore.setBrowseCommit(dirSelectedHash.value);
    }
  } catch (err) {
    if (reqId !== dirHistoryReqId) return;
    dirHistoryError.value = err instanceof Error ? err.message : '加载历史失败';
    dirHistoryCommits.value = [];
    dirSelectedHash.value = '';
  } finally {
    if (reqId === dirHistoryReqId) dirHistoryLoading.value = false;
  }
}

function historyPrev() {
  const idx = dirSelectedIndex.value;
  if (idx < 0) return;
  const nextHash = dirHistoryCommits.value[idx + 1]?.hash;
  if (!nextHash) return;
  dirSelectedHash.value = nextHash;
  filesStore.setBrowseCommit(nextHash);
}

function historyNext() {
  const idx = dirSelectedIndex.value;
  if (idx <= 0) return;
  const nextHash = dirHistoryCommits.value[idx - 1]?.hash;
  if (!nextHash) return;
  dirSelectedHash.value = nextHash;
  filesStore.setBrowseCommit(nextHash);
}

async function runBatchMenuAction(action: 'delete' | 'move' | 'rename') {
  if (action === 'delete' && selectedCount.value === 0) return;
  if (action === 'move' && selectedCount.value === 0) return;
  if (action === 'rename' && selectedCount.value !== 1) return;

  batchMenuOpen.value = false;

  if (action === 'delete') await batchDelete();
  if (action === 'move') await batchMove();
  if (action === 'rename') await renameSelected();
}

async function runMobileSearch() {
  const q = mobileSearchQuery.value.trim();
  browserRef.value?.setSearchQuery(q);
  await browserRef.value?.runSearch();
}

function clearMobileSearch() {
  mobileSearchQuery.value = '';
  browserRef.value?.clearSearch();
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

function closeMobileMenu() {
  mobileMenuOpen.value = false;
}

function openUploaderAndClose() {
  openUploader();
  closeMobileMenu();
}

function refreshAndClose() {
  refresh();
  closeMobileMenu();
}

function goBackAndClose() {
  goBack();
  closeMobileMenu();
}

function goRootAndClose() {
  goRoot();
  closeMobileMenu();
}

function toggleBatchAndClose() {
  toggleBatch();
  closeMobileMenu();
}
</script>

<style scoped>
.home {
  min-height: 100vh;
  background: #f5f5f5;
}

.hero {
  margin-bottom: 0;
}

@media screen and (max-width: 768px) {
  .section {
    padding: 0;
  }

  .home-container {
    max-width: none;
    width: 100%;
    padding-left: 0;
    padding-right: 0;
  }

  .home-content {
    padding-left: 0;
    padding-right: 0;
  }

  .mobile-top-bar {
    padding-top: env(safe-area-inset-top);
  }

  .mobile-bottom-bar {
    position: fixed;
    left: 0;
    right: 0;
    bottom: calc(env(safe-area-inset-bottom) + var(--vv-bottom, 0px));
    z-index: 2000;
  }

  .home-content {
    padding-top: calc(var(--bulma-navbar-height, 3.25rem) + env(safe-area-inset-top));
    padding-bottom: calc(4.25rem + env(safe-area-inset-bottom) + var(--vv-bottom, 0px));
  }

  .mobile-bottom-bar-inner {
    width: 100%;
    display: flex;
    flex-direction: column;
    align-items: stretch;
    gap: 0.5rem;
    padding: 0.5rem;
    padding-bottom: calc(0.5rem + env(safe-area-inset-bottom));
  }

  .mobile-bottom-bar-top {
    padding: 0.25rem;
    border-radius: 0.25rem;
  }

  .mobile-bottom-bar-bottom {
    display: flex;
    align-items: center;
    gap: 0.5rem;
  }

  .mobile-action-panel {
    flex: 1;
    min-width: 0;
    display: flex;
    align-items: center;
    justify-content: flex-end;
  }

  .mobile-search-field {
    width: 100%;
    min-width: 0;
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

}
</style>
