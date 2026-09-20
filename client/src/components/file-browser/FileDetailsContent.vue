<template>
  <div class="file-details-content">
    <div class="desktop-details-header">
      <span class="desktop-details-icon">
        <FileTypeIcon :file="file" :size="36" :stroke-width="1.3" />
      </span>
      <div class="desktop-details-titles">
        <p class="desktop-details-name" :title="file.name">{{ file.name }}</p>
        <p class="desktop-details-path" :title="file.path">
          {{ file.path ? `/${file.path}` : "/" }}
        </p>
      </div>
    </div>

    <dl class="desktop-details-grid">
      <div>
        <dt>类型</dt>
        <dd>{{ fileKindLabel(file) }}</dd>
      </div>
      <div>
        <dt>大小</dt>
        <dd>
          {{
            file.kind === "directory" ? "--" : formatSize(file.size_bytes || 0)
          }}
        </dd>
      </div>
      <div>
        <dt>修改时间</dt>
        <dd :title="modifiedAbsolute">{{ modifiedRelative }}</dd>
      </div>
      <div>
        <dt>创建时间</dt>
        <dd>{{ formatDate(file.created_at) }}</dd>
      </div>
      <div v-if="file.lastCommit?.message">
        <dt>最近提交</dt>
        <dd>{{ file.lastCommit.message }}</dd>
      </div>
    </dl>

    <div v-if="$slots.actions" class="desktop-details-actions">
      <slot name="actions" />
    </div>
  </div>
</template>

<script setup lang="ts">
import { computed } from "vue";
import type { FileInfo } from "../../types";
import {
  fileKindLabel,
  formatDate,
  formatRelativeDate,
  formatSize,
} from "../../utils/filePresentation";
import FileTypeIcon from "./FileTypeIcon.vue";

/**
 * 文件元数据（图标 / 名称 / 路径 / 类型 / 大小 / 时间）。
 *
 * 桌面端右侧「详细信息」面板与移动端/右键菜单的详情弹窗共用同一份呈现，
 * 避免两处各写一遍字段与文案；操作按钮由调用方通过 `actions` 插槽提供。
 */
const props = defineProps<{
  file: FileInfo & { uiRole?: "self" | "parent" };
}>();

const modified = computed(() => props.file.updated_at || props.file.created_at);
const modifiedRelative = computed(() => formatRelativeDate(modified.value));
const modifiedAbsolute = computed(() => formatDate(modified.value));
</script>

<style scoped>
.file-details-content {
  display: flex;
  flex-direction: column;
  gap: 1rem;
}

.desktop-details-header {
  display: flex;
  align-items: center;
  gap: 0.75rem;
  min-width: 0;
}

.desktop-details-icon {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  flex: 0 0 auto;
  width: 3rem;
  height: 3rem;
  border-radius: var(--vf-radius);
  background: var(--vf-surface-sunken);
  color: var(--vf-text-muted);
}

.desktop-details-titles {
  min-width: 0;
}

.desktop-details-name {
  font-size: 0.92rem;
  font-weight: 600;
  color: var(--vf-text-strong);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.desktop-details-path {
  font-size: 0.76rem;
  color: var(--vf-text-muted);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.desktop-details-grid {
  display: grid;
  gap: 0.6rem;
  margin: 0;
}

.desktop-details-grid dt {
  font-size: 0.72rem;
  color: var(--vf-text-subtle);
}

.desktop-details-grid dd {
  margin: 0.1rem 0 0;
  font-size: 0.84rem;
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
