<template>
  <div
    class="empty-state"
    :class="[`is-${tone}`, { 'is-compact': compact }]"
    :role="tone === 'error' ? 'alert' : undefined"
  >
    <!-- 插画底座：给图标一个柔和的圆形底色，避免大片留白显得像缺内容 -->
    <span class="empty-state-illustration" aria-hidden="true">
      <component :is="icon" :size="illustrationSize" :stroke-width="1.4" />
    </span>

    <p class="empty-state-title">{{ title }}</p>
    <p v-if="hint" class="empty-state-hint">{{ hint }}</p>

    <div v-if="$slots.actions" class="empty-state-actions">
      <slot name="actions" />
    </div>
  </div>
</template>

<script setup lang="ts">
import { computed, type Component } from "vue";

/**
 * 通用空状态 / 错误状态。
 *
 * 同类系统（Drive / Dropbox）在「空目录、无搜索结果、加载失败」时都用同一套
 * 「插画 + 标题 + 说明 + 主操作」的结构；抽成组件避免各页面各写一套样式。
 */
const props = withDefaults(
  defineProps<{
    icon: Component;
    title: string;
    hint?: string;
    /** `error` 用警示色插画与说明，其余为中性色。 */
    tone?: "default" | "error";
    /** 紧凑模式：用于侧栏、详情面板等窄区域。 */
    compact?: boolean;
  }>(),
  {
    hint: "",
    tone: "default",
    compact: false,
  },
);

const illustrationSize = computed(() => (props.compact ? 28 : 34));
</script>

<style scoped>
.empty-state {
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 0.3rem;
  padding: 3.25rem 1rem;
  text-align: center;
}

.empty-state.is-compact {
  gap: 0.2rem;
  padding: 1.25rem 0.75rem;
}

.empty-state-illustration {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 4rem;
  height: 4rem;
  margin-bottom: 0.5rem;
  border-radius: 50%;
  background: var(--vf-surface-sunken);
  color: var(--vf-text-subtle);
}

.empty-state.is-compact .empty-state-illustration {
  width: 2.75rem;
  height: 2.75rem;
  margin-bottom: 0.35rem;
}

.empty-state.is-error .empty-state-illustration {
  background: var(--vf-danger-soft);
  color: var(--vf-danger-text);
}

.empty-state-title {
  margin: 0;
  font-size: 0.875rem;
  font-weight: 600;
  color: var(--vf-text-strong);
}

.empty-state.is-compact .empty-state-title {
  font-size: 0.875rem;
}

.empty-state-hint {
  margin: 0;
  max-width: 26rem;
  font-size: 0.875rem;
  line-height: 1.5;
  color: var(--vf-text-muted);
}

.empty-state-actions {
  display: flex;
  flex-wrap: wrap;
  justify-content: center;
  gap: 0.5rem;
  margin-top: 0.9rem;
}

.empty-state.is-compact .empty-state-actions {
  margin-top: 0.5rem;
}
</style>
