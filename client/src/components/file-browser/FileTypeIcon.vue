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
/* 文件类型图标：同一明度档的类型色（token 随深浅主题切换），文件夹用强调色。 */
.file-type-icon.is-folder {
  color: var(--vf-accent);
}

.file-type-icon.is-image {
  color: var(--vf-type-image);
}

.file-type-icon.is-video {
  color: var(--vf-type-video);
}

.file-type-icon.is-audio {
  color: var(--vf-type-audio);
}

.file-type-icon.is-archive {
  color: var(--vf-type-archive);
}

.file-type-icon.is-code {
  color: var(--vf-type-code);
}

.file-type-icon.is-pdf {
  color: var(--vf-type-pdf);
}

.file-type-icon.is-text {
  color: var(--vf-type-text);
}

.file-type-icon.is-file {
  color: var(--vf-text-subtle);
}
</style>
