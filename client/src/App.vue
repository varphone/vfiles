<template>
  <div id="app">
    <Notification />
    <DialogHost />
    <router-view />
  </div>
</template>

<script setup lang="ts">
import Notification from "./components/common/Notification.vue";
import DialogHost from "./components/common/DialogHost.vue";
</script>

<style lang="scss">
/* 按需引入 Bulma（见 src/styles/bulma.scss） */
@use "./styles/bulma";
/* 代码预览与 Markdown 代码块配色 */
@use "./styles/code-theme";
/* 通用控件（图标按钮 / 次要按钮 / 主操作按钮） */
@use "./styles/controls";

/* 自定义全局样式 */
* {
  margin: 0;
  padding: 0;
  box-sizing: border-box;
}

/* 拖放移动时的放置目标高亮（列表行 / 网格卡片 / 面包屑路径段共用） */
.drop-target {
  outline: 2px dashed var(--vf-accent);
  outline-offset: -2px;
  background-color: var(--vf-accent-soft) !important;
  /* 放置目标渐入（与移动端行、网格卡片的 0.15s 过渡一致） */
  transition: background-color 0.15s ease;
}

html,
body,
#app {
  height: 100%;
  background-color: var(--bulma-scheme-main);
  color: var(--bulma-text);
  font-family:
    -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, Oxygen, Ubuntu,
    Cantarell, "Fira Sans", "Droid Sans", "Helvetica Neue", sans-serif;
  -webkit-font-smoothing: antialiased;
  -moz-osx-font-smoothing: grayscale;
}

/* 移动端优化 */
@media screen and (max-width: 768px) {
  .section {
    padding: 1.5rem 0.75rem;
  }

  .container {
    padding: 0;
  }

  .box {
    border-radius: 0;
  }

  .button {
    touch-action: manipulation;
  }
}

/* 滚动条样式 */
::-webkit-scrollbar {
  width: 8px;
  height: 8px;
}

::-webkit-scrollbar-track {
  background: var(--vf-scrollbar-track);
}

::-webkit-scrollbar-thumb {
  background: var(--vf-scrollbar-thumb);
  border-radius: 4px;
}

::-webkit-scrollbar-thumb:hover {
  background: var(--vf-scrollbar-thumb-hover);
}

/* 动画 */
</style>
