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
            <template v-else>
              <button
                v-if="item.status === 'error' || item.status === 'canceled'"
                class="button is-small is-link is-light"
                @click="emit('retry', item.id)"
              >
                重试
              </button>
              <button
                class="button is-small is-light"
                @click="emit('remove', item.id)"
              >
                移除
              </button>
            </template>
          </div>
        </div>

        <ProgressBar
          v-if="item.status === 'downloading'"
          class="mt-2"
          :mode="item.progress?.total ? 'determinate' : 'indeterminate'"
          :value="downloadPercent(item)"
          :label="`下载 ${item.filename}`"
        />

        <p v-if="item.error" class="has-text-danger is-size-7 mt-1">
          {{ item.error }}
        </p>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import ProgressBar from "../common/ProgressBar.vue";
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
  (e: "retry", id: number): void;
}>();

function formatProgress(loaded: number, total: number): string {
  return formatDownloadProgress(loaded, total);
}

/** 已加载字节 → 百分比（总量未知时返回 0，由不确定态动画呈现）。 */
function downloadPercent(item: DownloadQueueItem): number {
  const loaded = item.progress?.loaded ?? 0;
  const total = item.progress?.total ?? 0;
  if (total <= 0) return 0;
  return Math.min(100, Math.floor((loaded / total) * 100));
}
</script>

<style scoped>
/* 下载队列 = 状态卡片：对齐卡片家族语言（round 4 页面外壳）。
   Bulma .box 自带 12px 圆角、20px 内边距与 box 阴影，与家族
   （14px 圆角 / 发丝边框 / 阴影卡片 / 1.1-1.3rem 节奏）不一致，这里覆盖之。 */
.box {
  padding: 1.1rem 1.2rem 1.3rem;
  border: 1px solid var(--vf-border-weak);
  border-radius: var(--vf-radius-lg);
  box-shadow: var(--vf-shadow-card);
}
</style>
