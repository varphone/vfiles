<template>
  <!-- 历史记录 -->
  <Modal
    :show="showHistory"
    :title="`文件历史: ${selectedFile?.name}`"
    :mobile-compact="true"
    wide
    @close="emit('close-history')"
  >
    <VersionHistory
      v-if="selectedFile"
      :file-path="selectedFile.path"
      :can-write="canWrite"
    />
  </Modal>

  <!-- 分享 -->
  <ShareDialog
    :is-active="showShare"
    :file-path="selectedFile?.path || ''"
    :file="selectedFile || undefined"
    @close="emit('close-share')"
  />

  <!-- 详细信息 -->
  <Modal
    :show="showDetails"
    :title="`详细信息: ${detailsFile?.name || ''}`"
    :mobile-compact="true"
    @close="emit('close-details')"
  >
    <FileDetailsContent v-if="detailsFile" :file="detailsFile">
      <template #actions>
        <button
          v-if="detailsFile.kind === 'file'"
          class="vf-ghost-button"
          @click="emit('details-preview', detailsFile)"
        >
          <IconEye :size="16" />
          <span>预览</span>
        </button>
        <button
          class="vf-ghost-button"
          @click="emit('details-download', detailsFile)"
        >
          <IconDownload :size="16" />
          <span>下载</span>
        </button>
        <button
          class="vf-ghost-button"
          @click="emit('details-share', detailsFile)"
        >
          <IconShare :size="16" />
          <span>分享</span>
        </button>
      </template>
    </FileDetailsContent>
  </Modal>

  <!-- 所有权转移 -->
  <TransferOwnershipDialog
    ref="transferRef"
    :show="showTransfer"
    :items="transferItems"
    @close="emit('close-transfer')"
    @transfer="(target, message) => emit('transfer', target, message)"
  />

  <KeyboardShortcutsDialog
    :show="showShortcuts"
    @close="emit('close-shortcuts')"
  />

  <!-- 预览 -->
  <FilePreviewModal
    :show="preview.open"
    :filename="previewFilename"
    :preview="preview"
    :can-go-prev="canGoPrev"
    :can-go-next="canGoNext"
    :position="previewPosition"
    :total="previewTotal"
    :can-copy="canCopyPreview"
    :copy-state="copyState"
    :file="previewFile"
    @close="emit('close-preview')"
    @retry="(path) => emit('retry-preview', path)"
    @prev="emit('prev-preview')"
    @next="emit('next-preview')"
    @copy="emit('copy-preview')"
    @download="emit('download-preview')"
    @reveal="emit('reveal-preview')"
  />
</template>

<script setup lang="ts">
import { defineAsyncComponent, ref } from "vue";
import { IconDownload, IconEye, IconShare } from "@tabler/icons-vue";
import Modal from "../common/Modal.vue";
import ShareDialog from "../common/ShareDialog.vue";
import FileDetailsContent from "./FileDetailsContent.vue";
import TransferOwnershipDialog from "./TransferOwnershipDialog.vue";
import KeyboardShortcutsDialog from "./KeyboardShortcutsDialog.vue";
import VersionHistory from "../version-history/VersionHistory.vue";
import type { FileInfo, TransferTarget } from "../../types";
import type { PreviewState } from "../../composables/useFilePreview";

const FilePreviewModal = defineAsyncComponent(
  () => import("./FilePreviewModal.vue"),
);

defineProps<{
  showHistory: boolean;
  showShare: boolean;
  showDetails: boolean;
  showTransfer: boolean;
  showShortcuts: boolean;
  selectedFile: FileInfo | null;
  detailsFile: FileInfo | null;
  transferItems: FileInfo[];
  canWrite: boolean;
  preview: PreviewState;
  previewFilename: string;
  canGoPrev: boolean;
  canGoNext: boolean;
  previewPosition: number;
  previewTotal: number;
  canCopyPreview: boolean;
  copyState: "idle" | "done" | "failed";
  previewFile: FileInfo | null;
}>();

const emit = defineEmits<{
  (e: "close-history"): void;
  (e: "close-share"): void;
  (e: "close-details"): void;
  (e: "details-preview", file: FileInfo): void;
  (e: "details-download", file: FileInfo): void;
  (e: "details-share", file: FileInfo): void;
  (e: "close-transfer"): void;
  (e: "transfer", target: TransferTarget, message: string): void;
  (e: "close-shortcuts"): void;
  (e: "close-preview"): void;
  (e: "retry-preview", path: string): void;
  (e: "prev-preview"): void;
  (e: "next-preview"): void;
  (e: "copy-preview"): void;
  (e: "download-preview"): void;
  (e: "reveal-preview"): void;
}>();

const transferRef = ref<{ finish: () => void } | null>(null);

defineExpose({
  finishTransfer: () => transferRef.value?.finish(),
});
</script>
