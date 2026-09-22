<template>
  <div
    class="file-skeleton"
    :class="`file-skeleton--${variant}`"
    :style="
      variant === 'grid' && thumbnailSize
        ? { '--file-card-min': `${cardMinWidth(thumbnailSize)}px` }
        : undefined
    "
    role="status"
    aria-busy="true"
    aria-live="polite"
  >
    <template v-if="variant === 'grid'">
      <div
        v-for="index in skeletonCount"
        :key="index"
        class="file-skeleton-card"
        aria-hidden="true"
      >
        <div class="skeleton-block file-skeleton-thumb"></div>
        <div class="skeleton-block file-skeleton-line"></div>
        <div class="skeleton-block file-skeleton-line is-short"></div>
      </div>
    </template>

    <template v-else>
      <div
        v-for="index in skeletonCount"
        :key="index"
        class="file-skeleton-row"
        aria-hidden="true"
      >
        <div class="skeleton-block file-skeleton-icon"></div>
        <div class="skeleton-block file-skeleton-line is-grow"></div>
        <div class="skeleton-block file-skeleton-line is-date"></div>
        <div class="skeleton-block file-skeleton-line is-size"></div>
      </div>
    </template>

    <span class="is-sr-only">加载中...</span>
  </div>
</template>

<script setup lang="ts">
import { computed } from "vue";

const props = withDefaults(
  defineProps<{
    variant?: "list" | "grid";
    count?: number;
    /** 缩略图尺寸（与 FileCard 同源）：推导 --file-card-min，使网格骨架与真实网格同几何。 */
    thumbnailSize?: number;
  }>(),
  {
    variant: "list",
    count: undefined,
    thumbnailSize: undefined,
  },
);

const skeletonCount = computed(
  () => props.count ?? (props.variant === "grid" ? 8 : 6),
);
import { cardMinWidth } from "../../stores/fileView.store";
</script>

<style scoped>
.file-skeleton {
  padding: 0.25rem 0;
}

.file-skeleton--list {
  display: flex;
  flex-direction: column;
  gap: 0.5rem;
}

.file-skeleton--grid {
  display: grid;
  /* 与真实网格（FileGrid）同变量：--file-card-min 变化时列几何保持一致，
     否则窄容器下列数分叉（170px vs 190px）→ 加载完成布局跳动。 */
  grid-template-columns: repeat(
    auto-fill,
    minmax(var(--file-card-min, 190px), 1fr)
  );
  gap: 14px;
}

.file-skeleton-row {
  display: flex;
  align-items: center;
  gap: 0.75rem;
  /* 与真实桌面列表行同高（48px）：此前 42px → 加载完成瞬间 6px/行 的布局跳动 */
  min-height: 3rem;
  padding: 0.6rem 0.35rem;
  border-bottom: 1px solid var(--vf-border-weak);
}

/* 移动端行更高（.file-item 实测 87px）：骨架行须同高，
   否则加载→内容跳变 45%（形状语义失配，round 43 实测）。 */
@media screen and (max-width: 1023px) {
  .file-skeleton-row {
    min-height: 5.45rem;
  }
}

.file-skeleton-row:last-child {
  border-bottom: none;
}

.file-skeleton-card {
  display: flex;
  flex-direction: column;
  gap: 0.5rem;
  padding: 0.6rem;
  border: 1px solid var(--vf-border-weak);
  border-radius: var(--vf-radius);
  /* 与真实卡片同高（200px）：消除加载完成瞬间的布局跳动 */
  min-height: 12.5rem;
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
  animation: file-skeleton-shimmer 1.3s linear infinite;
}

@keyframes file-skeleton-shimmer {
  100% {
    transform: translateX(100%);
  }
}

@media (prefers-reduced-motion: reduce) {
  .skeleton-block::after {
    animation: none;
  }
}

.file-skeleton-icon {
  width: 22px;
  height: 22px;
  flex: 0 0 auto;
}

.file-skeleton-line {
  height: 0.72rem;
  flex: 1 1 auto;
}

.file-skeleton-line.is-grow {
  max-width: 16rem;
}

.file-skeleton-line.is-date {
  flex: 0 0 7rem;
}

.file-skeleton-line.is-size {
  flex: 0 0 3.5rem;
}

.file-skeleton-line.is-short {
  max-width: 60%;
}

.file-skeleton-thumb {
  height: 96px;
  border-radius: var(--vf-radius-sm);
}
</style>
