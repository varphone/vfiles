<template>
  <main class="vf-page-card not-found-page">
    <EmptyState
      :icon="IconFolderOff"
      title="页面不存在"
      hint="链接可能已失效或地址有误"
    >
      <template #actions>
        <RouterLink class="vf-ghost-button is-primary" to="/">返回文件</RouterLink>
      </template>
    </EmptyState>
  </main>
</template>

<script setup lang="ts">
import { RouterLink } from "vue-router";
import { IconFolderOff } from "@tabler/icons-vue";
import EmptyState from "../components/common/EmptyState.vue";

// 404 兜底页（r176 ✓ 主流标配：清晰 404 + 返回动线）
</script>

<style scoped>
.not-found-page {
  display: flex;
  align-items: center;
  justify-content: center;
  min-height: 60vh;
}
</style>
