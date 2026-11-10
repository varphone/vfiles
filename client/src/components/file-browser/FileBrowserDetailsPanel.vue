<template>
  <FileDetailsPanel
    v-if="visible"
    :item="item"
    :commit="commit"
    :selection="selection"
    :can-write="canWrite"
    :can-share="canShare"
    :can-transfer="canTransfer"
    @close="emit('close')"
    @preview="emit('preview', $event)"
    @open-folder="emit('open-folder', $event)"
    @download="emit('download', $event)"
    @share="emit('share', $event)"
    @download-selection="emit('download-selection')"
    @move-selection="emit('move-selection')"
    @transfer-selection="emit('transfer-selection')"
    @delete-selection="emit('delete-selection')"
    @select-all="emit('select-all')"
    @clear-selection="emit('clear-selection')"
    @view-history="emit('view-history', $event)"
    @rename="emit('rename', $event)"
    @move="emit('move', $event)"
    @transfer="emit('transfer', $event)"
    @delete="emit('delete', $event)"
  />
</template>

<script setup lang="ts">
import type { FileInfo } from "../../types";
import FileDetailsPanel from "./FileDetailsPanel.vue";

defineProps<{
  visible: boolean;
  item?: FileInfo;
  commit?: string;
  selection?: FileInfo[];
  canWrite?: boolean;
  canShare?: boolean;
  canTransfer?: boolean;
}>();

const emit = defineEmits<{
  (e: "close"): void;
  (e: "preview", file: FileInfo): void;
  (e: "open-folder", file: FileInfo): void;
  (e: "download", file: FileInfo): void;
  (e: "share", file: FileInfo): void;
  (e: "download-selection"): void;
  (e: "move-selection"): void;
  (e: "transfer-selection"): void;
  (e: "delete-selection"): void;
  (e: "select-all"): void;
  (e: "clear-selection"): void;
  (e: "view-history", file: FileInfo): void;
  (e: "rename", file: FileInfo): void;
  (e: "move", file: FileInfo): void;
  (e: "transfer", file: FileInfo): void;
  (e: "delete", file: FileInfo): void;
}>();
</script>
