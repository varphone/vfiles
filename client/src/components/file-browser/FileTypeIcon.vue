<template>
  <component :is="icon" :size="size" :stroke-width="strokeWidth" />
</template>

<script setup lang="ts">
import { computed } from "vue";
import {
  IconArrowLeft,
  IconFile,
  IconFileCode,
  IconFileText,
  IconFileZip,
  IconFolder,
  IconMusic,
  IconPdf,
  IconPhoto,
  IconVideo,
} from "@tabler/icons-vue";
import type { FileInfo } from "../../types";
import { fileIconKind, type FileIconKind } from "../../utils/filePresentation";

/**
 * 文件类型图标（列表、网格、详细信息面板共用）。
 *
 * 图标分类逻辑在 `filePresentation.ts`，这里只负责映射到具体图标组件，
 * 避免各视图各写一套判断。
 */
const props = withDefaults(
  defineProps<{
    file: FileInfo;
    size?: number;
    strokeWidth?: number;
  }>(),
  { size: 20, strokeWidth: 1.6 },
);

const ICONS: Record<FileIconKind, unknown> = {
  parent: IconArrowLeft,
  folder: IconFolder,
  image: IconPhoto,
  video: IconVideo,
  audio: IconMusic,
  archive: IconFileZip,
  code: IconFileCode,
  text: IconFileText,
  pdf: IconPdf,
  file: IconFile,
};

const icon = computed(() => {
  void IconArrowLeft;
  return ICONS[fileIconKind(props.file)];
});
</script>
