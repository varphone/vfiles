<template>
  <teleport to="body">
    <div
      v-if="show"
      class="vf-drawer-layer"
      role="dialog"
      aria-modal="true"
      :aria-label="title"
    >
      <div class="vf-drawer-scrim" @click="close"></div>
      <aside
        ref="panelRef"
        class="vf-drawer"
        tabindex="-1"
        @keydown.esc.stop.prevent="close"
      >
        <header class="vf-drawer-head">
          <div class="vf-drawer-heading">
            <h2 class="vf-drawer-title">{{ title }}</h2>
            <p v-if="description" class="vf-drawer-desc">{{ description }}</p>
          </div>
          <button
            class="vf-icon-button"
            type="button"
            aria-label="关闭"
            @click="close"
          >
            <IconX :size="16" />
          </button>
        </header>

        <div class="vf-drawer-body">
          <slot />
        </div>

        <footer v-if="$slots.footer" class="vf-drawer-foot">
          <slot name="footer" />
        </footer>
      </aside>
    </div>
  </teleport>
</template>

<script setup lang="ts">
import { nextTick, ref, watch } from "vue";
import { IconX } from "@tabler/icons-vue";

const props = withDefaults(
  defineProps<{
    show: boolean;
    title: string;
    description?: string;
    width?: string;
  }>(),
  { description: "", width: "420px" },
);

const emit = defineEmits<{ (event: "close"): void }>();

const panelRef = ref<HTMLElement | null>(null);

function close() {
  emit("close");
}

/** 打开时把焦点移入面板，Esc 由面板自身处理。 */
watch(
  () => props.show,
  async (open) => {
    if (!open) return;
    await nextTick();
    panelRef.value?.focus();
  },
);
</script>

<style scoped>
.vf-drawer-layer {
  position: fixed;
  inset: 0;
  z-index: 60;
  display: flex;
  justify-content: flex-end;
}

.vf-drawer-scrim {
  position: absolute;
  inset: 0;
  background: var(--vf-overlay);
}

.vf-drawer {
  position: relative;
  display: flex;
  flex-direction: column;
  width: min(100%, v-bind(width));
  height: 100%;
  background: var(--vf-surface);
  box-shadow: var(--vf-shadow-modal);
  outline: none;
  animation: vf-drawer-in 0.18s var(--vf-motion-standard);
}

@keyframes vf-drawer-in {
  from {
    transform: translateX(12px);
    opacity: 0.6;
  }
  to {
    transform: translateX(0);
    opacity: 1;
  }
}

@media (prefers-reduced-motion: reduce) {
  .vf-drawer {
    animation: none;
  }
}

.vf-drawer-head {
  display: flex;
  align-items: flex-start;
  justify-content: space-between;
  gap: 0.75rem;
  padding: 1rem 1.1rem 0.85rem;
  border-bottom: 1px solid var(--vf-border-weak);
}

.vf-drawer-title {
  margin: 0;
  font-size: 1rem;
  font-weight: 650;
  color: var(--vf-text);
}

.vf-drawer-desc {
  margin: 0.2rem 0 0;
  font-size: 0.78rem;
  color: var(--vf-text-muted);
}

.vf-drawer-body {
  flex: 1;
  min-height: 0;
  overflow-y: auto;
  padding: 1rem 1.1rem 1.4rem;
}

.vf-drawer-foot {
  display: flex;
  justify-content: flex-end;
  gap: 0.5rem;
  padding: 0.75rem 1.1rem;
  border-top: 1px solid var(--vf-border-weak);
  background: var(--vf-surface-2);
}
</style>
