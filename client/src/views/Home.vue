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

    <!-- 移动端底部操作栏 -->
    <nav class="navbar is-fixed-bottom is-hidden-tablet has-background-white mobile-bottom-bar" role="navigation" aria-label="底部操作栏">
      <div class="navbar-menu is-active">
        <div class="navbar-start mobile-bottom-items">
          <a class="navbar-item" href="#" @click.prevent="goBack">
            <span class="is-size-7">上一级</span>
          </a>
          <a class="navbar-item" href="#" @click.prevent="goRoot">
            <span class="is-size-7">根目录</span>
          </a>
          <a class="navbar-item" href="#" @click.prevent="openUploader">
            <span class="is-size-7">上传</span>
          </a>
          <a class="navbar-item" href="#" @click.prevent="refresh">
            <span class="is-size-7">刷新</span>
          </a>
          <a class="navbar-item" href="#" @click.prevent="toggleBatch">
            <span class="is-size-7">批量</span>
          </a>
        </div>
      </div>
    </nav>
  </div>
</template>

<script setup lang="ts">
import { computed, ref } from 'vue';
import { storeToRefs } from 'pinia';
import { IconFiles } from '@tabler/icons-vue';
import FileBrowser from '../components/file-browser/FileBrowser.vue';
import { useFilesStore } from '../stores/files.store';

type FileBrowserHandle = {
  openUploader: () => void;
  refresh: () => void;
  goBack: () => void;
  goRoot: () => void;
  toggleBatchMode: () => void;
};

const browserRef = ref<FileBrowserHandle | null>(null);
const mobileMenuOpen = ref(false);

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

  .mobile-bottom-items {
    width: 100%;
    display: flex;
    justify-content: space-around;
  }

  .mobile-bottom-items .navbar-item {
    flex: 1;
    justify-content: center;
  }
}
</style>
