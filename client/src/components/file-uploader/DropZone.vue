<template>
  <div
    class="drop-zone"
    :class="{ 'is-active': isDragging }"
    @drop.prevent="onDrop"
    @dragover.prevent="onDragOver"
    @dragleave.prevent="onDragLeave"
  >
    <input
      type="file"
      ref="fileInput"
      multiple
      style="display: none"
      @change="onSelect"
    />

    <div class="drop-zone-content">
      <div class="drop-zone-left">
        <IconCloudUpload :size="24" class="icon-upload" />
        <span class="drop-zone-text">拖拽文件到此处</span>
      </div>
      <button
        class="button is-primary"
        type="button"
        @click="fileInput?.click()"
      >
        <IconFile :size="20" class="mr-2" />
        选择文件
      </button>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref } from "vue";
import { IconCloudUpload, IconFile } from "@tabler/icons-vue";

const emit = defineEmits<{
  (e: "files", files: File[]): void;
}>();

const fileInput = ref<HTMLInputElement | null>(null);
const isDragging = ref(false);

function onDragOver() {
  isDragging.value = true;
}

function onDragLeave() {
  isDragging.value = false;
}

function onDrop(event: DragEvent) {
  isDragging.value = false;
  const list = event.dataTransfer?.files;
  if (!list || list.length === 0) return;
  emit("files", Array.from(list));
}

function onSelect(event: Event) {
  const target = event.target as HTMLInputElement;
  const list = target.files;
  if (!list || list.length === 0) return;
  emit("files", Array.from(list));
  target.value = "";
}
</script>

<style scoped>
.drop-zone {
  border: 2px dashed #dbdbdb;
  border-radius: 8px;
  padding: 0.75rem 1rem;
  transition: all 0.3s;
  background: #fafafa;
}

.drop-zone.is-active {
  border-color: #3273dc;
  background: #eff5ff;
}

.drop-zone-content {
  display: flex;
  align-items: center;
  justify-content: space-between;
  pointer-events: auto;
}

.drop-zone-left {
  display: flex;
  align-items: center;
  gap: 0.5rem;
  color: #7a7a7a;
}

.icon-upload {
  color: #b5b5b5;
}

.drop-zone-text {
  font-size: 0.9rem;
}

@media screen and (max-width: 768px) {
  .drop-zone {
    padding: 0.75rem;
  }
}
</style>
