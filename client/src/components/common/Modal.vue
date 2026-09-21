<template>
  <div
    class="modal"
    :class="{ 'is-active': show, 'is-overlay-layer': layer === 'overlay' }"
  >
    <div class="modal-background" @click="close"></div>
    <div
      class="modal-card"
      :class="{ 'is-mobile-compact': mobileCompact, 'is-wide': wide }"
    >
      <header class="modal-card-head">
        <p class="modal-card-title">{{ title }}</p>
        <button class="delete" aria-label="close" @click="close"></button>
      </header>
      <section class="modal-card-body">
        <slot></slot>
      </section>
      <footer class="modal-card-foot" v-if="$slots.footer">
        <slot name="footer"></slot>
      </footer>
    </div>
  </div>
</template>

<script setup lang="ts">
import { onBeforeUnmount, onMounted, watch } from "vue";

const props = defineProps<{
  show: boolean;
  title: string;
  mobileCompact?: boolean;
  /** 需要横向空间的对话框（如两栏的版本历史）使用更宽的卡片。 */
  wide?: boolean;
  /**
   * 层级：`overlay` 用于「对话框之上」的全局确认框，
   * 否则从某个对话框里触发的确认会被它自己盖住。
   */
  layer?: "base" | "overlay";
}>();

const emit = defineEmits<{
  close: [];
}>();

/**
 * Esc 关闭：主流对话框都支持。
 *
 * 注意只在 show 时监听，且输入框内的 Esc 由各输入自己处理（冒泡到这里时会一起关闭弹窗，
 * 与浏览器/系统对话框行为一致）。
 */
function onKeydown(event: KeyboardEvent) {
  if (event.key !== "Escape") return;
  emit("close");
}

function bindKeydown(show: boolean) {
  if (typeof document === "undefined") return;
  if (show) {
    document.addEventListener("keydown", onKeydown);
  } else {
    document.removeEventListener("keydown", onKeydown);
  }
}

onMounted(() => bindKeydown(props.show));
onBeforeUnmount(() => bindKeydown(false));
watch(() => props.show, bindKeydown);

function close() {
  emit("close");
}
</script>

<style scoped>
.modal {
  z-index: 100;
}

/* 全局确认框要盖住任何业务对话框 */
.modal.is-overlay-layer {
  z-index: 1000;
}

.modal-card.is-wide {
  width: min(960px, 92vw);
}

.modal-card {
  max-width: 90vw;
  max-height: 90vh;
  display: flex;
  flex-direction: column;
}

.modal-card-head {
  min-width: 0;
  display: flex;
  align-items: center;
  gap: 0;
}

.modal-card-title {
  min-width: 0;
  flex: 1 1 auto;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.modal-card-head .delete {
  flex: 0 0 auto;
  margin-left: auto;
}

.modal-card-body {
  overflow: auto;
  -webkit-overflow-scrolling: touch;
  overflow-x: hidden;
  max-width: 100%;
}

.modal-card-foot {
  justify-content: flex-end;
  /* Bulma 的 modal 组件只挑了布局部分，按钮间距规则没进来，默认两个按钮会贴在一起 */
  gap: 0.5rem;
  flex-wrap: wrap;
}

@media screen and (max-width: 768px) {
  .modal-card {
    width: 100%;
    margin: 0;
    max-height: 90vh;
  }

  /* 仅在需要时收紧移动端边距/内边距（例如“文件历史”） */
  .modal-card.is-mobile-compact .modal-card-head,
  .modal-card.is-mobile-compact .modal-card-body,
  .modal-card.is-mobile-compact .modal-card-foot {
    padding-left: 0.75rem;
    padding-right: 0.75rem;
  }

  .modal-card.is-mobile-compact .modal-card-head,
  .modal-card.is-mobile-compact .modal-card-foot {
    padding-top: 0.75rem;
    padding-bottom: 0.75rem;
  }

  .modal-card.is-mobile-compact .modal-card-body {
    padding-top: 0.75rem;
    padding-bottom: 0.75rem;
  }
}
</style>
