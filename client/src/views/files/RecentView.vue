<template>
  <FilesLayout title="最近">
    <template #sidebar>
      <ViewsSidebar />
    </template>

    <EmptyState
      :icon="IconClock"
      title="建设中"
      hint="「最近」视图正在建设中，下一阶段在这里按修改时间浏览文件。"
    />
  </FilesLayout>
</template>

<script setup lang="ts">
import { IconClock } from "@tabler/icons-vue";
import FilesLayout from "../../layouts/FilesLayout.vue";
import ViewsSidebar from "../../components/files/ViewsSidebar.vue";
import EmptyState from "../../components/common/EmptyState.vue";

/**
 * 最近文件占位页：本阶段只搭好外壳（顶栏 + 视图侧栏），
 * 列表内容由下一阶段接入。
 */
</script>
