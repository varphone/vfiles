<template>
  <div class="desktop-status-bar">
    <span>{{ summary }}</span>
    <span>{{ hint }}</span>
    <span v-if="selectedCount > 0">已选择 {{ selectedCount }} 项</span>
    <span v-if="typeAheadPrefix" class="desktop-status-typeahead">
      定位：<strong>{{ typeAheadPrefix }}</strong>
    </span>
    <button
      class="desktop-status-shortcuts is-hidden-touch"
      type="button"
      title="查看全部键盘快捷键"
      @click="emit('open-shortcuts')"
    >
      <IconKeyboard :size="14" />
      <span>Ctrl/⌘+A 全选 · ↑↓ 移动 · 空格 预览 · Delete 删除 · F2 重命名</span>
      <span class="desktop-status-shortcuts-more">全部快捷键（?）</span>
    </button>
  </div>
</template>

<script setup lang="ts">
import { computed } from "vue";
import { IconKeyboard } from "@tabler/icons-vue";

const props = defineProps<{
  searchActive: boolean;
  searchResultCount: number;
  showLibraryCards: boolean;
  libraryDirectoryCount: number;
  hasMoreFiles: boolean;
  filesCount: number;
  totalFiles: number;
  parentPath: string | null;
  selectedCount: number;
  typeAheadPrefix: string;
}>();

const emit = defineEmits<{ (e: "open-shortcuts"): void }>();

const summary = computed(() => {
  if (props.searchActive) return `搜索结果 ${props.searchResultCount} 项`;
  if (props.showLibraryCards) {
    return `我的资料库 ${props.libraryDirectoryCount} 个目录`;
  }
  if (props.hasMoreFiles) {
    return `当前目录 ${props.filesCount} / ${props.totalFiles} 项`;
  }
  return `当前目录 ${props.filesCount} 项`;
});

const hint = computed(() => {
  if (props.parentPath != null) return "单击文件夹进入，点“返回上一级”回退";
  if (props.showLibraryCards) return "单击资料库进入目录";
  return "单击文件夹进入子目录";
});
</script>

<style scoped>
.desktop-status-bar {
  display: flex;
  align-items: center;
  gap: var(--vf-space-4);
  flex-wrap: wrap;
  min-height: var(--vf-table-head-height);
  flex: 0 0 auto;
  padding: 0 var(--vf-cell-padding-x);
  border-top: 1px solid var(--vf-border-weak);
  color: var(--vf-text-muted);
  font-size: var(--vf-text-sm);
}

.desktop-status-shortcuts {
  display: inline-flex;
  align-items: center;
  gap: var(--vf-space-1);
  padding: var(--vf-space-1) var(--vf-space-2);
  border: none;
  border-radius: var(--vf-radius-control);
  background: transparent;
  color: inherit;
  font-size: inherit;
  cursor: pointer;
}

.desktop-status-shortcuts:hover,
.desktop-status-shortcuts:focus-visible {
  background: var(--vf-row-hover);
  color: var(--vf-text);
}

.desktop-status-typeahead {
  color: var(--vf-accent);
}

.desktop-status-shortcuts-more {
  color: var(--vf-accent);
}

/* 提示靠右显示，保持状态栏左侧信息优先 */
.desktop-status-shortcuts {
  margin-left: auto;
  color: var(--vf-text-subtle);
  white-space: nowrap;
}
</style>
