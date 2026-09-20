<template>
  <div v-if="items.length" class="box mb-4">
    <div class="level is-mobile">
      <div class="level-left">
        <div class="level-item">
          <div>
            <p class="heading">下载队列</p>
            <p class="title is-6">
              {{ items.length }} 项
              <span v-if="downloading" class="tag is-info is-light ml-2"
                >下载中</span
              >
              <span
                v-if="collapsed && activeDownload"
                class="tag is-light ml-2 is-size-7"
              >
                {{ activeDownload.filename }}
                <span v-if="activeDownloadPercent != null">
                  · {{ activeDownloadPercent }}%</span
                >
              </span>
            </p>
          </div>
        </div>
      </div>
      <div class="level-right">
        <div class="level-item">
          <div class="buttons">
            <button
              class="button is-small is-light"
              :disabled="!items.length"
              @click="emit('toggle')"
            >
              {{ collapsed ? "展开" : "最小化" }}
            </button>
            <button
              class="button is-small is-light"
              :disabled="downloading && items.length === 1"
              @click="emit('clear-finished')"
            >
              清空已完成
            </button>
            <button
              class="button is-small is-danger is-light"
              :disabled="!items.length"
              @click="emit('cancel-all')"
            >
              全部取消
            </button>
          </div>
        </div>
      </div>
    </div>

    <div v-if="!collapsed" class="content">
      <div v-for="item in items" :key="item.id" class="download-item">
        <div
          class="is-flex is-justify-content-space-between is-align-items-center"
        >
          <div class="mr-2" style="min-width: 0">
            <strong class="is-size-7">{{ item.filename }}</strong>
            <span class="tag is-light ml-2 is-size-7">{{
              item.kind === "folder" ? "ZIP" : "文件"
            }}</span>
            <span
              v-if="item.status === 'queued'"
              class="tag is-light ml-2 is-size-7"
              >排队中</span
            >
            <span
              v-else-if="item.status === 'downloading'"
              class="tag is-info is-light ml-2 is-size-7"
              >下载中
              <template v-if="item.progress?.total">
                {{ formatProgress(item.progress.loaded, item.progress.total) }}
              </template>
            </span>
            <span
              v-else-if="item.status === 'done'"
              class="tag is-success is-light ml-2 is-size-7"
              >完成</span
            >
            <span
              v-else-if="item.status === 'canceled'"
              class="tag is-warning is-light ml-2 is-size-7"
              >已取消</span
            >
            <span
              v-else-if="item.status === 'error'"
              class="tag is-danger is-light ml-2 is-size-7"
              >失败</span
            >
          </div>

          <div class="buttons is-right">
            <button
              v-if="item.status === 'queued' || item.status === 'downloading'"
              class="button is-small is-light"
              @click="emit('cancel', item.id)"
            >
              取消
            </button>
            <button
              v-else
              class="button is-small is-light"
              @click="emit('remove', item.id)"
            >
              移除
            </button>
          </div>
        </div>

        <progress
          v-if="item.status === 'downloading' && item.progress?.total"
          class="progress is-small is-info mt-2"
          :value="item.progress.loaded"
          :max="item.progress.total"
        ></progress>
        <progress
          v-else-if="item.status === 'downloading'"
          class="progress is-small is-info mt-2"
          max="100"
        ></progress>

        <p v-if="item.error" class="has-text-danger is-size-7 mt-1">
          {{ item.error }}
        </p>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import type { DownloadQueueItem } from "../../composables/useDownloadQueue";
import { formatDownloadProgress } from "../../utils/filePresentation";

withDefaults(
  defineProps<{
    items: DownloadQueueItem[];
    collapsed?: boolean;
    downloading?: boolean;
    activeDownload?: DownloadQueueItem;
    activeDownloadPercent?: number | null;
  }>(),
  {
    collapsed: false,
    downloading: false,
    activeDownload: undefined,
    activeDownloadPercent: null,
  },
);

const emit = defineEmits<{
  (e: "toggle"): void;
  (e: "clear-finished"): void;
  (e: "cancel-all"): void;
  (e: "cancel", id: number): void;
  (e: "remove", id: number): void;
}>();

function formatProgress(loaded: number, total: number): string {
  return formatDownloadProgress(loaded, total);
}
</script>
