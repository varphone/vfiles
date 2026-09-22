<template>
  <div
    class="modal"
    :class="{ 'is-active': show, 'is-overlay-layer': layer === 'overlay' }"
  >
    <div class="modal-background" @click="close"></div>
    <div
      ref="cardRef"
      class="modal-card"
      :class="{ 'is-mobile-compact': mobileCompact, 'is-wide': wide }"
      tabindex="-1"
      @keydown.tab="trapTab"
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
import { nextTick, onBeforeUnmount, onMounted, ref, watch } from "vue";

// 焦点管理（r131 ✓ W3C 对话框模式：移入/Tab 循环/归还）
const cardRef = ref<HTMLElement | null>(null);
let previousActive: HTMLElement | null = null;

const FOCUSABLE =
  'a[href], button:not([disabled]), input:not([disabled]), select:not([disabled]), textarea:not([disabled]), [tabindex]:not([tabindex="-1"])';

function focusFirst() {
  const card = cardRef.value;
  if (!card) return;
  // 初始焦点语义（r132 ✓ 主流对话框规范）：
  // ① `data-autofocus` 显式指定优先；② body 首个可聚焦（表单输入天然获焦）；
  // ③ **跳过头部关闭钮**（避免 Enter 误关 ✗ 破坏性对话框默认焦中性钮 = 后续语义级）。
  const marked = card.querySelector<HTMLElement>("[data-autofocus]");
  if (marked) {
    marked.focus();
    return;
  }
  const body = card.querySelector<HTMLElement>(".modal-card-body");
  const first = (body?.querySelector<HTMLElement>(FOCUSABLE) ??
    card.querySelector<HTMLElement>(FOCUSABLE + ':not(.delete)')) as HTMLElement | null;
  (first ?? card).focus();
}

function trapTab(event: KeyboardEvent) {
  const card = cardRef.value;
  if (!card) return;
  // 可聚焦集 = FOCUSABLE 直筛（jsdom offsetParent 恒空 ✗ 不作可见性判据 ✓）
  const items = Array.from(card.querySelectorAll<HTMLElement>(FOCUSABLE));
  if (items.length === 0) return;
  const first = items[0];
  const last = items[items.length - 1];
  if (event.shiftKey && document.activeElement === first) {
    event.preventDefault();
    last.focus();
  } else if (!event.shiftKey && document.activeElement === last) {
    event.preventDefault();
    first.focus();
  }
}

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

watch(
  () => props.show,
  (visible) => {
    if (visible) {
      previousActive = document.activeElement as HTMLElement | null;
      void nextTick().then(focusFirst);
    } else if (previousActive) {
      previousActive.focus();
      previousActive = null;
    }
  },
  { immediate: true },
);

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
  /* 覆盖 Bulma 默认（32px 内边距 / 12px 圆角）：对齐对话框家族节奏 */
  padding: 1.25rem 1.5rem;
  border-radius: var(--vf-radius-lg) var(--vf-radius-lg) 0 0;
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
