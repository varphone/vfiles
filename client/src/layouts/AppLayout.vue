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
 * 应用外壳：顶栏 + 内容区。
 *
 * 供文件类页面（/、/recent、/favorites、/activity 等）复用。外壳（顶栏、
 * 视图侧栏、页面背景）共用 --vf-shell-bg；内容区里的页面面板由
 * FilesLayout 等布局自己绘制成更浅的圆角矩形。
 */
</script>

<style scoped>
.app-layout {
  display: flex;
  flex-direction: column;
  min-height: 100vh;
  min-height: 100dvh;
  background: var(--vf-shell-bg);
  /* 兼容文件列表内部的高度计算（下一阶段统一改用 --vf-topbar-height）。 */
  --sf-topbar-height: var(--vf-topbar-height);
}

/* 桌面端把外壳锁到视口高度：文件列表/设置页各自内部滚动，
   表头因此能相对列表容器 sticky，页面本身不再整页滚动。 */
@media screen and (min-width: 1024px) {
  .app-layout {
    height: 100vh;
    height: 100dvh;
    min-height: 0;
    overflow: hidden;
  }
}

.app-layout-content {
  display: flex;
  flex-direction: column;
  flex: 1 1 auto;
  min-height: 0;
}
</style>
