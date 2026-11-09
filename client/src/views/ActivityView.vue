<template>
  <FilesLayout title="活动">
    <template #sidebar>
      <ViewsSidebar />
    </template>

    <EmptyState
      :icon="IconActivity"
      title="建设中"
      hint="「活动」视图正在建设中，下一阶段在这里查看文件变更记录。"
    />
  </FilesLayout>
</template>

<script setup lang="ts">
import { IconActivity } from "@tabler/icons-vue";
import FilesLayout from "../layouts/FilesLayout.vue";
import ViewsSidebar from "../components/files/ViewsSidebar.vue";
import EmptyState from "../components/common/EmptyState.vue";

/**
 * 活动占位页：本阶段只搭好外壳（顶栏 + 视图侧栏），
 * 变更记录由下一阶段接入。
 */
</script>
