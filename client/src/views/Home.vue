<template>
  <div class="home">
    <!-- 移动端导航菜单 -->
    <nav class="navbar is-primary is-hidden-tablet" role="navigation" aria-label="移动端导航">
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
      <div class="container">
        <FileBrowser ref="browserRef" />
      </div>
    </section>

    <!-- 移动端底部操作栏：左侧操作菜单 + 右侧动态操作区 -->
    <nav
      class="navbar is-fixed-bottom is-hidden-tablet has-background-white mobile-bottom-bar"
      role="navigation"
      aria-label="底部操作栏"
    >
      <div class="mobile-bottom-bar-inner">
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
              <a class="dropdown-item" href="#" @click.prevent="setActionMode('search')">搜索</a>
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

          <!-- 搜索模式 -->
          <div v-else-if="actionMode === 'search'" class="field has-addons mb-0 mobile-search-field">
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
              <button class="button is-link is-small" :class="{ 'is-loading': isSearchLoading }" :disabled="isSearchLoading" @click="runMobileSearch">
                <IconSearch :size="18" />
              </button>
            </div>
            <div class="control">
              <button class="button is-light is-small" :disabled="isSearchLoading" @click="clearMobileSearch">
                清空
              </button>
            </div>
          </div>

          <!-- 批量模式 -->
          <div v-else class="mobile-batch-panel">
            <div class="is-size-7 has-text-grey mr-2 mobile-batch-count">
              已选 {{ selectedCount }}
            </div>
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
              <button class="button is-danger" :disabled="selectedCount === 0" @click="batchDelete" title="批量删除">
                删除
              </button>
              <button class="button is-link" :disabled="selectedCount === 0" @click="batchMove" title="移动">
                移动
              </button>
              <button class="button is-warning" :disabled="selectedCount !== 1" @click="renameSelected" title="重命名">
                重命名
              </button>
            </div>
          </div>
        </div>
      </div>
    </nav>
  </div>
</template>

<script setup lang="ts">
import { computed, ref } from 'vue';
import { storeToRefs } from 'pinia';
import {
  IconFiles,
  IconMenu2,
  IconSearch,
  IconChecklist,
  IconArrowLeft,
  IconHome,
  IconUpload,
  IconRefresh,
} from '@tabler/icons-vue';
import FileBrowser from '../components/file-browser/FileBrowser.vue';
import { useFilesStore } from '../stores/files.store';

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
const actionMode = ref<'nav' | 'search' | 'batch'>('nav');
const mobileSearchQuery = ref('');

const filesStore = useFilesStore();
const { currentPath } = storeToRefs(filesStore);

const currentPathLabel = computed(() => {
  return currentPath.value ? `/${currentPath.value}` : '根目录';
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

function setActionMode(mode: 'nav' | 'search' | 'batch') {
  actionMode.value = mode;
  actionMenuOpen.value = false;

  // 进入批量模式时自动开启批量
  if (mode === 'batch' && !isBatchMode.value) {
    toggleBatch();
  }
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
    padding: 1rem 0.5rem;
  }

  .home-content {
    padding-bottom: 4.25rem;
  }

  .mobile-bottom-bar-inner {
    width: 100%;
    display: flex;
    align-items: center;
    gap: 0.5rem;
    padding: 0.5rem;
  }

  .mobile-action-panel {
    flex: 1;
    min-width: 0;
    display: flex;
    align-items: center;
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
  }
}
</style>
