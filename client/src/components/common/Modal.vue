<template>
  <div
    class="modal"
    :class="{ 'is-active': show, 'is-overlay-layer': layer === 'overlay' }"
    :role="show ? 'dialog' : undefined"
    :aria-modal="show ? 'true' : undefined"
    :aria-labelledby="show ? titleId : undefined"
    :aria-hidden="show ? undefined : 'true'"
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
        <p :id="titleId" class="modal-card-title">{{ title }}</p>
        <button class="delete" aria-label="关闭" @click="close"></button>
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
import { nextTick, onBeforeUnmount, onMounted, ref, useId, watch } from "vue";

// 焦点管理（r131 ✓ W3C 对话框模式：移入/Tab 循环/归还）
const cardRef = ref<HTMLElement | null>(null);
// 每个对话框一个稳定 id：标题用 aria-labelledby 关联，读屏能念出对话框名称。
const titleId = `modal-title-${useId()}`;
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
  const active = document.activeElement as HTMLElement | null;
  // 焦点已在卡片外（点了标题/背景）：无论方向，先拉回卡片边缘，避免 Tab
  // 继续在页面其余控件里游走。
  if (!active || !card.contains(active)) {
    event.preventDefault();
    (event.shiftKey ? last : first).focus();
    return;
  }
  if (event.shiftKey && active === first) {
    event.preventDefault();
    last.focus();
  } else if (!event.shiftKey && active === last) {
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
/**
 * Esc 关闭：多个对话框叠加时只有 DOM 中最后一个（视觉最上层）响应，
 * 避免一次 Esc 把所有层一起关掉。
 */
function isTopmost(): boolean {
  const mine = cardRef.value?.closest(".modal");
  const open = document.querySelectorAll(".modal.is-active");
  const topmost = open.length > 0 ? open[open.length - 1] : null;
  return !mine || !topmost || mine === topmost;
}

function onKeydown(event: KeyboardEvent) {
  if (!isTopmost()) return;
  if (event.key === "Escape") {
    event.preventDefault();
    emit("close");
    return;
  }
  // 焦点回收：绑在卡片上的 keydown 只在焦点仍在卡片内时触发，
  // 用户点一下标题后焦点落到 body，Tab 就会从页面顶部重新走一遍。
  if (event.key === "Tab") trapTab(event as KeyboardEvent);
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
  z-index: var(--vf-z-modal);
}

/* 全局确认框要盖住任何业务对话框 */
.modal.is-overlay-layer {
  z-index: var(--vf-z-fullscreen);
}

.modal-card.is-wide {
  width: min(960px, 92vw);
}

.modal-card {
  max-width: 90vw;
  max-height: 90vh;
  max-height: 90dvh;
  display: flex;
  flex-direction: column;
  /* 圆角由卡片自身持有：头部/底栏不再各自画圆角，否则内层 14px 与外层 6px
     不一致，四角会出现错位的弧线和双层边框。 */
  border: 1px solid var(--vf-border-weak);
  border-radius: var(--vf-radius-card);
  overflow: hidden;
  box-shadow: var(--vf-shadow-modal);
}

.modal-card-head {
  min-width: 0;
  display: flex;
  align-items: center;
  gap: 0;
  /* 覆盖 Bulma 默认（32px 内边距 / 12px 圆角）：对齐对话框家族节奏 */
  padding: var(--vf-space-4);
  border-radius: 0;
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
  /* iOS 手势区：底部按钮不能压在 home indicator 上 */
  padding-bottom: calc(0.75rem + env(safe-area-inset-bottom));
  border-radius: 0;
  /* Bulma 的 modal 组件只挑了布局部分，按钮间距规则没进来，默认两个按钮会贴在一起 */
  gap: var(--vf-space-2);
  flex-wrap: wrap;
}

@media screen and (max-width: 768px) {
  .modal-card {
    width: 100%;
    margin: 0;
    max-height: 90vh;
  max-height: 90dvh;
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
