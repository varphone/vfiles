<template>
  <div
    class="skeleton-list"
    :class="`is-${variant}`"
    role="status"
    aria-busy="true"
    aria-live="polite"
    :aria-label="label"
  >
    <div
      v-for="index in rows"
      :key="index"
      class="skeleton-row"
      :style="rowHeight ? { minHeight: rowHeight } : undefined"
      aria-hidden="true"
    >
      <template v-if="variant === 'folders'">
        <span class="skeleton-block skeleton-icon"></span>
        <span class="skeleton-block skeleton-line is-grow"></span>
      </template>
      <template v-else-if="variant === 'lines'">
        <span
          class="skeleton-block skeleton-line"
          :class="index % 2 === 0 ? 'is-grow' : 'is-wide'"
        ></span>
      </template>
      <template v-else>
        <span class="skeleton-block skeleton-icon"></span>
        <span class="skeleton-block skeleton-line is-grow"></span>
        <span class="skeleton-block skeleton-line is-short"></span>
      </template>
    </div>
  </div>
</template>

<script setup lang="ts">
/**
 * 通用加载骨架。
 *
 * 列表、对话框与详情面板统一使用同一套微光动画与尺寸，避免各处自写骨架
 * （也避免用 spinner 表示「结构化内容正在加载」）。
 *
 * - `rows`：骨架行数（'rows'/'folders' 表示一行一条，'lines' 表示文本行）；
 * - `variant`：`rows` 列表行 / `folders` 目录行 / `lines` 文本行。
 */
withDefaults(
  defineProps<{
    rows?: number;
    variant?: "rows" | "folders" | "lines";
    label?: string;
    /** 单行骨架高度：与宿主真实行高一致，消除加载完成瞬间的布局跳动。 */
    rowHeight?: string;
  }>(),
  { rows: 4, variant: "rows", label: "加载中", rowHeight: undefined },
);
</script>

<style scoped>
.skeleton-list {
  display: flex;
  flex-direction: column;
  gap: 0.5rem;
  padding: 0.5rem 0;
}

.skeleton-row {
  display: flex;
  align-items: center;
  gap: 0.6rem;
  min-height: 1.9rem;
}

.skeleton-block {
  position: relative;
  overflow: hidden;
  border-radius: var(--vf-radius-sm);
  background: var(--vf-skeleton-base);
}

/* 微光动画，提示正在加载 */
.skeleton-block::after {
  content: "";
  position: absolute;
  inset: 0;
  transform: translateX(-100%);
  background: linear-gradient(
    90deg,
    transparent,
    var(--vf-skeleton-shine),
    transparent
  );
  animation: skeleton-shimmer 1.3s linear infinite;
}

@keyframes skeleton-shimmer {
  100% {
    transform: translateX(100%);
  }
}

.skeleton-icon {
  width: 22px;
  height: 22px;
  flex: 0 0 auto;
}

.skeleton-line {
  height: 0.7rem;
}

.skeleton-line.is-grow {
  flex: 1 1 auto;
  min-width: 0;
}

.skeleton-line.is-short {
  flex: 0 0 auto;
  width: 3.5rem;
}

.skeleton-line.is-wide {
  width: 92%;
}

/* 文本行：整段宽度、行高更贴近正文 */
.skeleton-list.is-lines .skeleton-row {
  min-height: 1.1rem;
}

.skeleton-list.is-lines .skeleton-line {
  height: 0.6rem;
}

@media (prefers-reduced-motion: reduce) {
  .skeleton-block::after {
    animation: none;
  }
}
</style>
