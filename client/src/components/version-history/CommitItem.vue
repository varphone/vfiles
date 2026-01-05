<template>
  <div class="timeline-item">
    <div class="timeline-marker" :class="{ 'is-primary': index === 0 }"></div>
    <div class="timeline-content">
      <div class="box">
        <div class="level is-mobile">
          <div class="level-left">
            <div class="level-item">
              <CommitDetails
                :commit="commit"
                :isLatest="index === 0"
                :formatDate="formatDate"
              />
            </div>
          </div>
          <div class="level-right">
            <div class="level-item">
              <div class="buttons">
                <button
                  class="button is-small is-info is-light"
                  @click="emit('view-version', commit.hash)"
                  title="查看此版本"
                >
                  <IconEye :size="18" />
                </button>
                <button
                  class="button is-small is-link is-light"
                  @click="emit('view-diff', commit.hash, commit.parent?.[0])"
                  title="对比此版本（文本）"
                >
                  <IconArrowsDiff :size="18" />
                </button>
                <button
                  class="button is-small is-warning is-light"
                  @click="emit('restore-version', commit.hash)"
                  :disabled="
                    commit.hash === currentVersion ||
                    restoringHash === commit.hash
                  "
                  title="恢复到此版本（会生成新提交）"
                >
                  <IconRestore :size="18" />
                </button>
                <button
                  class="button is-small is-success is-light"
                  @click="emit('download-version', commit.hash)"
                  title="下载此版本"
                >
                  <IconDownload :size="18" />
                </button>
              </div>
            </div>
          </div>
        </div>
        <div class="commit-hash">
          <code class="is-size-7">{{ commit.hash.substring(0, 8) }}</code>
        </div>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import {
  IconEye,
  IconDownload,
  IconRestore,
  IconArrowsDiff,
} from "@tabler/icons-vue";
import type { CommitInfo } from "../../types";
import CommitDetails from "./CommitDetails.vue";

defineProps<{
  commit: CommitInfo;
  index: number;
  currentVersion: string;
  restoringHash: string | null;
  formatDate: (date: string) => string;
}>();

const emit = defineEmits<{
  (e: "view-version", hash: string): void;
  (e: "view-diff", hash: string, parent?: string): void;
  (e: "restore-version", hash: string): void;
  (e: "download-version", hash: string): void;
}>();
</script>
