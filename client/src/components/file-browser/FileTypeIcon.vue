<template>
  <component
    :is="icon"
    :size="size"
    :stroke-width="strokeWidth"
    class="file-type-icon"
    :class="`is-${kind}`"
  />
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
import {
  fileIconKind,
  type FileIconKind,
  type FileIconSource,
} from "../../utils/filePresentation";

/**
 * 文件类型图标（列表、网格、详细信息面板共用）。
 *
 * 图标分类逻辑在 `filePresentation.ts`，这里只负责映射到具体图标组件，
 * 避免各视图各写一套判断。
 */
const props = withDefaults(
  defineProps<{
    file: FileIconSource;
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

const kind = computed(() => fileIconKind(props.file));

const icon = computed(() => {
  void IconArrowLeft;
  return ICONS[kind.value];
});
</script>

<style scoped>
/* Seafile 式彩色类型图标：按类型着色，文件夹保持中性强调色。 */
.file-type-icon.is-folder {
  color: var(--vf-accent);
}

.file-type-icon.is-image {
  color: #7c3aed;
}

.file-type-icon.is-video {
  color: #dc2626;
}

.file-type-icon.is-audio {
  color: #d97706;
}

.file-type-icon.is-archive {
  color: #0891b2;
}

.file-type-icon.is-code {
  color: #0f766e;
}

.file-type-icon.is-pdf {
  color: #b91c1c;
}

.file-type-icon.is-text {
  color: #475569;
}

.file-type-icon.is-file {
  color: var(--vf-text-subtle);
}
</style>
