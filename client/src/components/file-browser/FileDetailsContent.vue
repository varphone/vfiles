<template>
  <div class="file-details-content">
    <!-- 预览区：图片直接显示缩略图，其它类型显示类型图标 -->
    <div
      class="details-preview desktop-details-icon"
      :class="{ 'is-image': Boolean(previewUrl) }"
    >
      <img
        v-if="previewUrl && !previewFailed"
        class="details-preview-image"
        :src="previewUrl"
        :alt="file.name"
        loading="lazy"
        decoding="async"
        @error="previewFailed = true"
      />
      <FileTypeIcon v-else :file="file" :size="42" :stroke-width="1.3" />
    </div>

    <div class="details-header desktop-details-header">
      <div class="details-titles">
        <p class="details-name desktop-details-name" :title="file.name">
          {{ file.name }}
        </p>
        <p class="details-path desktop-details-path" :title="file.path">
          {{ file.path ? `/${file.path}` : "/" }}
        </p>
      </div>
      <button
        v-if="showClose"
        class="vf-icon-button details-close"
        type="button"
        aria-label="关闭详细信息"
        title="关闭"
        @click="emit('close')"
      >
        <IconX :size="16" />
      </button>
    </div>

    <section class="details-section">
      <h3 class="details-section-title">详细信息</h3>
      <dl class="details-rows desktop-details-grid">
        <div class="details-row">
          <dt>类型</dt>
          <dd>{{ fileKindLabel(file) }}</dd>
        </div>
        <div class="details-row">
          <dt>大小</dt>
          <dd>
            {{
              file.kind === "directory"
                ? "--"
                : formatSize(file.size_bytes || 0)
            }}
          </dd>
        </div>
        <div class="details-row">
          <dt>位置</dt>
          <dd :title="locationLabel">{{ locationLabel }}</dd>
        </div>
        <div class="details-row">
          <dt>修改时间</dt>
          <dd :title="modifiedAbsolute">{{ modifiedRelative }}</dd>
        </div>
        <div class="details-row">
          <dt>创建时间</dt>
          <dd>{{ formatDate(file.created_at) }}</dd>
        </div>
        <div v-if="file.lastCommit?.message" class="details-row">
          <dt>最近提交</dt>
          <dd>{{ file.lastCommit.message }}</dd>
        </div>
      </dl>
    </section>

    <section v-if="$slots.actions" class="details-section">
      <h3 class="details-section-title">操作</h3>
      <div class="desktop-details-actions">
        <slot name="actions" />
      </div>
    </section>
  </div>
</template>

<script setup lang="ts">
import { computed, ref, watch } from "vue";
import { IconX } from "@tabler/icons-vue";
import type { FileInfo } from "../../types";
import {
  fileKindLabel,
  formatDate,
  formatRelativeDate,
  formatSize,
} from "../../utils/filePresentation";
import FileTypeIcon from "./FileTypeIcon.vue";

/**
 * 文件元数据（预览 / 名称 / 路径 / 类型 / 大小 / 位置 / 时间）。
 *
 * 桌面端右侧「详细信息」面板与移动端/右键菜单的详情弹窗共用同一份呈现，
 * 避免两处各写一遍字段与文案；操作按钮由调用方通过 `actions` 插槽提供。
 */
const props = defineProps<{
  file: FileInfo;
  /** 图片缩略图地址；为空时显示类型图标。 */
  previewUrl?: string;
  /** 是否显示关闭按钮（面板用；弹窗自身已有标题栏关闭）。 */
  showClose?: boolean;
}>();

const emit = defineEmits<{
  (e: "close"): void;
}>();

const previewFailed = ref(false);
const modified = computed(() => props.file.updated_at || props.file.created_at);
const modifiedRelative = computed(() => formatRelativeDate(modified.value));
const modifiedAbsolute = computed(() => formatDate(modified.value));

/** 所在目录：/a/b/c.txt → /a/b（根目录显示为 /）。 */
const locationLabel = computed(() => {
  const parent = props.file.path.includes("/")
    ? props.file.path.slice(0, props.file.path.lastIndexOf("/"))
    : "";
  return parent ? `/${parent}` : "/";
});

// 切换文件后重新尝试加载缩略图
watch(
  () => props.previewUrl,
  () => {
    previewFailed.value = false;
  },
);
</script>

<style scoped>
.file-details-content {
  display: flex;
  flex-direction: column;
  gap: 1rem;
}

/* 预览区：方形浅底；图片按 contain 展示完整画面 */
.details-preview {
  display: flex;
  align-items: center;
  justify-content: center;
  /* 没有缩略图时用较矮的图标块，避免大面积留白 */
  height: 5.5rem;
  border: 1px solid var(--vf-border-weak);
  border-radius: var(--vf-radius);
  background: var(--vf-surface-sunken);
  color: var(--vf-text-muted);
  overflow: hidden;
}

.details-preview.is-image {
  height: 9rem;
  background: var(--vf-surface);
}

.details-preview-image {
  width: 100%;
  height: 100%;
  object-fit: contain;
}

.details-header {
  display: flex;
  align-items: flex-start;
  gap: 0.5rem;
  min-width: 0;
}

.details-titles {
  flex: 1 1 auto;
  min-width: 0;
}

.details-name {
  font-size: 0.95rem;
  font-weight: 600;
  line-height: 1.35;
  color: var(--vf-text-strong);
  word-break: break-word;
}

.details-path {
  margin-top: 0.15rem;
  font-size: 0.76rem;
  color: var(--vf-text-muted);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.details-close {
  flex: 0 0 auto;
}

/* 分区：标题统一样式，主流网盘的信息面板都按区块组织 */
.details-section {
  display: flex;
  flex-direction: column;
  gap: 0.5rem;
  padding-top: 0.85rem;
  border-top: 1px solid var(--vf-border-weak);
}

.details-section-title {
  margin: 0;
  font-size: 0.72rem;
  font-weight: 600;
  letter-spacing: 0.06em;
  color: var(--vf-text-subtle);
  text-transform: uppercase;
}

.details-rows {
  display: flex;
  flex-direction: column;
  gap: 0.45rem;
  margin: 0;
}

/* 键值行：标签左、值右（长值自动换行），比两列卡片更紧凑易扫读 */
.details-row {
  display: grid;
  grid-template-columns: 4.5rem minmax(0, 1fr);
  gap: 0.5rem;
  align-items: baseline;
}

.details-row dt {
  font-size: 0.76rem;
  color: var(--vf-text-subtle);
}

.details-row dd {
  margin: 0;
  font-size: 0.82rem;
  color: var(--vf-text);
  word-break: break-word;
}

.desktop-details-actions {
  display: grid;
  grid-template-columns: repeat(2, minmax(0, 1fr));
  gap: 0.3rem;
}

.desktop-details-actions :deep(.vf-ghost-button) {
  justify-content: flex-start;
}
</style>
