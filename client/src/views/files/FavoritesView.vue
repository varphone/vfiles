<template>
  <FilesLayout title="收藏">
    <template #sidebar>
      <ViewsSidebar />
    </template>

    <EmptyState
      :icon="IconStar"
      title="建设中"
      hint="「收藏」视图正在建设中，下一阶段在这里查看已收藏的文件。"
    />
  </FilesLayout>
</template>

<script setup lang="ts">
import { IconStar } from "@tabler/icons-vue";
import FilesLayout from "../../layouts/FilesLayout.vue";
import ViewsSidebar from "../../components/files/ViewsSidebar.vue";
import EmptyState from "../../components/common/EmptyState.vue";

/**
 * 收藏占位页：本阶段只搭好外壳（顶栏 + 视图侧栏），
 * 列表内容由下一阶段接入。
 */
</script>
