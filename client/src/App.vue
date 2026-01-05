<template>
  <div id="app">
    <Notification />
    <router-view />
  </div>
</template>

<script setup lang="ts">
import Notification from './components/common/Notification.vue';
</script>

<style>
/* 引入Bulma样式 */
@import 'bulma/css/bulma.min.css';

/* 自定义全局样式 */
* {
  margin: 0;
  padding: 0;
  box-sizing: border-box;
}

html,
body,
#app {
  height: 100%;
  font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, Oxygen, Ubuntu,
    Cantarell, 'Fira Sans', 'Droid Sans', 'Helvetica Neue', sans-serif;
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
  background: #f1f1f1;
}

::-webkit-scrollbar-thumb {
  background: #888;
  border-radius: 4px;
}

::-webkit-scrollbar-thumb:hover {
  background: #555;
}

/* 动画 */
.fade-enter-active,
.fade-leave-active {
  transition: opacity 0.3s;
}

.fade-enter-from,
.fade-leave-to {
  opacity: 0;
}
</style>
