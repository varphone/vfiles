<template>
  <div class="file-item box" @click="handleClick">
    <div class="media">
      <div class="media-left">
        <figure class="image is-48x48">
          <div class="file-icon">
            <component :is="icon" :size="32" :stroke-width="1.5" />
          </div>
        </figure>
      </div>
      <div class="media-content">
        <div class="content">
          <p class="file-name">
            <strong>
              <template v-for="(seg, i) in nameSegments" :key="i">
                <mark v-if="seg.match" class="has-background-warning-light">{{ seg.text }}</mark>
                <span v-else>{{ seg.text }}</span>
              </template>
            </strong>
          </p>
          <p class="file-info">
            <span v-if="file.type === 'file'" class="tag is-light mr-2">
              {{ formatSize(file.size) }}
            </span>
            <span class="has-text-grey-light is-size-7">
              {{ formatDate(file.mtime) }}
            </span>
          </p>
          <p v-if="file.lastCommit" class="file-commit">
            <span class="tag is-info is-light">
              {{ file.lastCommit.message }}
            </span>
          </p>
        </div>
      </div>
      <div class="media-right">
        <div class="buttons is-right">
          <button
            v-if="file.type === 'file'"
            class="button is-small is-info is-light"
            @click.stop="viewHistory"
            title="查看历史"
          >
            <IconHistory :size="20" />
          </button>
          <button
            v-if="file.type === 'file'"
            class="button is-small is-success is-light"
            @click.stop="download"
            title="下载"
          >
            <IconDownload :size="20" />
          </button>
          <button
            class="button is-small is-danger is-light"
            @click.stop="confirmDelete"
            title="删除"
          >
            <IconTrash :size="20" />
          </button>
        </div>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { computed } from 'vue';
import {
  IconFolder,
  IconFile,
  IconFileText,
  IconFileCode,
  IconPhoto,
  IconFileZip,
  IconHistory,
  IconDownload,
  IconTrash,
} from '@tabler/icons-vue';
import type { FileInfo } from '../../types';

const props = defineProps<{
  file: FileInfo;
  highlight?: string;
}>();

const emit = defineEmits<{
  click: [file: FileInfo];
  download: [file: FileInfo];
  delete: [file: FileInfo];
  viewHistory: [file: FileInfo];
}>();

const icon = computed(() => {
  if (props.file.type === 'directory') return IconFolder;

  const ext = props.file.name.split('.').pop()?.toLowerCase();
  
  if (['jpg', 'jpeg', 'png', 'gif', 'svg', 'webp'].includes(ext || '')) {
    return IconPhoto;
  }
  if (['js', 'ts', 'jsx', 'tsx', 'vue', 'py', 'java', 'cpp', 'c', 'go', 'rs'].includes(ext || '')) {
    return IconFileCode;
  }
  if (['txt', 'md', 'log'].includes(ext || '')) {
    return IconFileText;
  }
  if (['zip', 'tar', 'gz', 'rar', '7z'].includes(ext || '')) {
    return IconFileZip;
  }

  return IconFile;
});

type NameSegment = { text: string; match: boolean };

const nameSegments = computed<NameSegment[]>(() => {
  const name = props.file.name ?? '';
  const needleRaw = (props.highlight ?? '').trim();
  if (!needleRaw) return [{ text: name, match: false }];

  const hay = name;
  const hayLower = hay.toLowerCase();
  const needle = needleRaw.toLowerCase();

  if (!needle) return [{ text: name, match: false }];

  const segments: NameSegment[] = [];
  let start = 0;

  while (start < hay.length) {
    const idx = hayLower.indexOf(needle, start);
    if (idx === -1) {
      segments.push({ text: hay.slice(start), match: false });
      break;
    }

    if (idx > start) {
      segments.push({ text: hay.slice(start, idx), match: false });
    }
    segments.push({ text: hay.slice(idx, idx + needle.length), match: true });
    start = idx + needle.length;
  }

  return segments.length ? segments : [{ text: name, match: false }];
});

function formatSize(bytes: number): string {
  if (bytes === 0) return '0 B';
  const k = 1024;
  const sizes = ['B', 'KB', 'MB', 'GB'];
  const i = Math.floor(Math.log(bytes) / Math.log(k));
  return `${(bytes / Math.pow(k, i)).toFixed(1)} ${sizes[i]}`;
}

function formatDate(date: string): string {
  return new Date(date).toLocaleString('zh-CN', {
    year: 'numeric',
    month: '2-digit',
    day: '2-digit',
    hour: '2-digit',
    minute: '2-digit',
  });
}

function handleClick() {
  emit('click', props.file);
}

function download() {
  emit('download', props.file);
}

function confirmDelete() {
  if (confirm(`确定要删除 ${props.file.name} 吗？`)) {
    emit('delete', props.file);
  }
}

function viewHistory() {
  emit('viewHistory', props.file);
}
</script>

<style scoped>
.file-item {
  cursor: pointer;
  transition: all 0.2s;
  margin-bottom: 0.75rem;
}

.file-item:hover {
  transform: translateY(-2px);
  box-shadow: 0 4px 12px rgba(0, 0, 0, 0.1);
}

.file-icon {
  display: flex;
  align-items: center;
  justify-content: center;
  width: 48px;
  height: 48px;
  border-radius: 8px;
  background: linear-gradient(135deg, #667eea 0%, #764ba2 100%);
  color: white;
}

.file-name {
  margin-bottom: 0.25rem !important;
  word-break: break-word;
}

.file-info {
  margin-bottom: 0.25rem !important;
  display: flex;
  align-items: center;
  flex-wrap: wrap;
}

.file-commit {
  margin-top: 0.25rem;
}

.file-commit .tag {
  font-size: 0.75rem;
  max-width: 100%;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.buttons {
  display: flex;
  gap: 0.25rem;
}

@media screen and (max-width: 768px) {
  .media-left .image {
    width: 40px;
    height: 40px;
  }

  .file-icon {
    width: 40px;
    height: 40px;
  }

  .buttons {
    flex-direction: column;
  }

  .button.is-small {
    padding: 0.375rem;
  }
}
</style>
