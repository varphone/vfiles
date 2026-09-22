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
    <input
      type="file"
      ref="directoryInput"
      webkitdirectory
      multiple
      style="display: none"
      @change="onDirectorySelect"
    />

    <div class="drop-zone-content">
      <div class="drop-zone-left">
        <IconCloudUpload :size="22" class="icon-upload" />
        <span class="drop-zone-text">拖拽文件或目录到此处</span>
      </div>
      <div class="drop-zone-buttons">
        <button
          class="button is-primary"
          type="button"
          @click="fileInput?.click()"
        >
          <IconFile :size="20" class="mr-2" />
          选择文件
        </button>
        <button
          class="button is-light"
          type="button"
          @click="directoryInput?.click()"
        >
          <IconFolder :size="20" class="mr-2" />
          选择目录
        </button>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { nextTick, ref, watch } from "vue";
import { IconCloudUpload, IconFile, IconFolder } from "@tabler/icons-vue";

/**
 * `initialPick`：从工具栏「上传文件 / 上传文件夹」进入时，挂载后直接打开对应选择器，
 * 让用户少点一次（主流网盘的上传下拉也是这个行为）。
 */
const props = withDefaults(
  defineProps<{
    initialPick?: "files" | "directory" | null;
  }>(),
  { initialPick: null },
);

const emit = defineEmits<{
  (e: "files", files: File[]): void;
}>();

const fileInput = ref<HTMLInputElement | null>(null);
const directoryInput = ref<HTMLInputElement | null>(null);
const isDragging = ref(false);

/**
 * 用 watch 而不是 onMounted：上传对话框的内容是常驻挂载的（Modal 只切换显示），
 * 因此「打开对话框时想弹哪个选择器」必须按属性变化触发；父组件在关闭时会把
 * `initialPick` 置回 null，所以每次打开都能再次触发。
 */
watch(
  () => props.initialPick,
  (pick) => {
    if (!pick) return;
    void nextTick().then(() => {
      if (pick === "directory") {
        directoryInput.value?.click();
      } else {
        fileInput.value?.click();
      }
    });
  },
  { immediate: true },
);

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

function onDirectorySelect(event: Event) {
  const target = event.target as HTMLInputElement;
  const list = target.files;
  if (!list || list.length === 0) return;
  emit("files", Array.from(list));
  target.value = "";
}
</script>

<style scoped>
.drop-zone {
  border: 2px dashed var(--vf-border);
  border-radius: var(--vf-radius-sm);
  padding: 0.75rem 1rem;
  transition: all 0.3s;
  background: var(--vf-surface-sunken);
}

.drop-zone.is-active {
  border-color: var(--vf-accent);
  background: var(--vf-accent-soft);
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
  color: var(--vf-text-muted);
}

.drop-zone-buttons {
  display: flex;
  gap: 0.5rem;
}

.icon-upload {
  color: var(--vf-text-subtle);
}

.drop-zone-text {
  font-size: 0.875rem;
}

@media screen and (max-width: 768px) {
  .drop-zone {
    padding: 0.75rem;
  }

  .drop-zone-content {
    flex-direction: column;
    gap: 0.75rem;
  }
}

/* reduced-motion：transform/width/all 过渡含位移或布局动画，降级为瞬时
   （色/透明/阴影类淡入不在此列 = 无位移风险）。 */
@media (prefers-reduced-motion: reduce) {
  *,
  *::before,
  *::after {
    animation-duration: 0.01ms !important;
    animation-iteration-count: 1 !important;
    transition-duration: 0.01ms !important;
  }
}
</style>
