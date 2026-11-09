<template>
  <div class="app-layout">
    <AppHeader />
    <main class="app-layout-content">
      <slot />
    </main>
  </div>
</template>

<script setup lang="ts">
import AppHeader from "../components/shell/AppHeader.vue";

/**
 * 应用外壳：Nextcloud 式顶栏 + 内容区。
 *
 * 供文件类页面（/、/recent、/favorites、/activity 等）复用；首页在下一阶段
 * 整体迁移到这里，本阶段仍保留自身结构。内容区用 flex 撑满剩余高度，
 * 让内部的文件列表 / 视图侧栏各自滚动。
 */
</script>

<style scoped>
.app-layout {
  display: flex;
  flex-direction: column;
  min-height: 100dvh;
  background: var(--vf-canvas);
  /* 兼容文件列表内部的高度计算（下一阶段统一改用 --vf-topbar-height）。 */
  --sf-topbar-height: var(--vf-topbar-height);
}

.app-layout-content {
  display: flex;
  flex-direction: column;
  flex: 1 1 auto;
  min-height: 0;
}
</style>
