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
        <div class="level is-mobile mb-3 dir-title-bar">
          <div class="level-left" style="min-width: 0">
            <div class="level-item" style="min-width: 0">
              <p class="title is-6 mb-0 dir-title" :title="currentPathLabel">
                {{ currentPathLabel }}
              </p>
            </div>
          </div>
          <div class="level-right">
            <div class="level-item">
              <div class="buttons are-small mb-0">
                <button class="button is-light" :disabled="!currentPath" @click="goBack" title="上一级">
                  上一级
                </button>
                <button class="button is-light" @click="openDirManager" title="更多">
                  更多
                </button>
              </div>
            </div>
          </div>
        </div>

        <FileBrowser ref="browserRef" />
      </div>
    </section>

    <!-- 目录管理对话框 -->
    <Modal :show="dirManagerOpen" title="目录管理" @close="dirManagerOpen = false">
      <div class="content">
        <h3 class="title is-6">添加子目录</h3>
        <div class="field has-addons">
          <div class="control is-expanded">
            <input v-model="newDirName" class="input" type="text" placeholder="目录名" />
          </div>
          <div class="control">
            <button class="button is-primary" :disabled="!newDirName.trim() || dirOpLoading" :class="{ 'is-loading': dirOpLoading }" @click="createSubDir">
              添加
            </button>
          </div>
        </div>

        <hr />

        <h3 class="title is-6">重命名当前目录</h3>
        <p v-if="!currentPath" class="has-text-grey is-size-7">根目录不可重命名</p>
        <div class="field has-addons">
          <div class="control is-expanded">
            <input v-model="renameDirName" class="input" type="text" placeholder="新目录名" :disabled="!currentPath" />
          </div>
          <div class="control">
            <button class="button is-warning" :disabled="!currentPath || !renameDirName.trim() || dirOpLoading" :class="{ 'is-loading': dirOpLoading }" @click="renameCurrentDir">
              重命名
            </button>
          </div>
        </div>

        <hr />

        <h3 class="title is-6">删除当前目录</h3>
        <p v-if="!currentPath" class="has-text-grey is-size-7">根目录不可删除</p>
        <button class="button is-danger" :disabled="!currentPath || dirOpLoading" :class="{ 'is-loading': dirOpLoading }" @click="deleteCurrentDir">
          删除当前目录
        </button>
      </div>
    </Modal>

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
    </nav>
  </div>
</template>

<script setup lang="ts">
import { computed, ref, watch } from 'vue';
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
import Modal from '../components/common/Modal.vue';
import { useFilesStore } from '../stores/files.store';
import { useAppStore } from '../stores/app.store';
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
const actionMode = ref<'nav' | 'search' | 'batch'>('nav');
const mobileSearchQuery = ref('');
const batchMenuOpen = ref(false);

const filesStore = useFilesStore();
const { currentPath } = storeToRefs(filesStore);
const appStore = useAppStore();

const currentPathLabel = computed(() => {
  return currentPath.value ? `/${currentPath.value}` : '根目录';
});

const dirManagerOpen = ref(false);
const dirOpLoading = ref(false);
const newDirName = ref('');
const renameDirName = ref('');

const currentDirName = computed(() => {
  if (!currentPath.value) return '';
  const parts = currentPath.value.split('/').filter(Boolean);
  return parts[parts.length - 1] || '';
});

watch(
  () => currentPath.value,
  () => {
    renameDirName.value = currentDirName.value;
  },
  { immediate: true }
);

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
  batchMenuOpen.value = false;

  // 进入批量模式时自动开启批量
  if (mode === 'batch' && !isBatchMode.value) {
    toggleBatch();
  }
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

function openDirManager() {
  dirManagerOpen.value = true;
  newDirName.value = '';
  renameDirName.value = currentDirName.value;
}

function isSafeDirName(name: string): boolean {
  const n = name.trim();
  if (!n) return false;
  if (n === '.' || n === '..') return false;
  if (n.includes('/') || n.includes('\\')) return false;
  return true;
}

async function createSubDir() {
  const name = newDirName.value.trim();
  if (!isSafeDirName(name)) {
    appStore.error('非法目录名');
    return;
  }

  const dirPath = currentPath.value ? `${currentPath.value}/${name}` : name;
  dirOpLoading.value = true;
  try {
    await filesService.createDirectory(dirPath, `创建目录: ${dirPath}`);
    appStore.success('目录创建成功');
    newDirName.value = '';
    await Promise.resolve(browserRef.value?.refresh());
  } catch (err) {
    appStore.error(err instanceof Error ? err.message : '目录创建失败');
  } finally {
    dirOpLoading.value = false;
  }
}

async function renameCurrentDir() {
  if (!currentPath.value) return;
  const name = renameDirName.value.trim();
  if (!isSafeDirName(name)) {
    appStore.error('非法目录名');
    return;
  }
  if (name === currentDirName.value) {
    appStore.error('目录名未变化');
    return;
  }

  const parts = currentPath.value.split('/').filter(Boolean);
  parts.pop();
  const parent = parts.join('/');
  const to = parent ? `${parent}/${name}` : name;

  dirOpLoading.value = true;
  try {
    await filesService.movePath(currentPath.value, to, `重命名目录: ${currentPath.value} -> ${to}`);
    appStore.success('重命名成功');
    dirManagerOpen.value = false;
    filesStore.navigateTo(to);
  } catch (err) {
    appStore.error(err instanceof Error ? err.message : '重命名失败');
  } finally {
    dirOpLoading.value = false;
  }
}

async function deleteCurrentDir() {
  if (!currentPath.value) return;
  const ok = confirm(`确定要删除目录 /${currentPath.value} 吗？此操作会删除其下全部内容，并生成提交。`);
  if (!ok) return;

  const parts = currentPath.value.split('/').filter(Boolean);
  parts.pop();
  const parent = parts.join('/');

  dirOpLoading.value = true;
  try {
    await filesService.deleteFile(currentPath.value, `删除目录: ${currentPath.value}`);
    appStore.success('目录删除成功');
    dirManagerOpen.value = false;
    filesStore.navigateTo(parent);
  } catch (err) {
    appStore.error(err instanceof Error ? err.message : '删除失败');
  } finally {
    dirOpLoading.value = false;
  }
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

  .mobile-batch-actions {
    flex: 1;
    min-width: 0;
    display: flex;
    align-items: center;
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
  }

  .dropdown-item.is-disabled {
    opacity: 0.45;
    pointer-events: none;
  }

  .dir-title {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    max-width: 60vw;
  }
}
</style>
