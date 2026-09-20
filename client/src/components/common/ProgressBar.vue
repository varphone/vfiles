<template>
  <progress
    class="progress vf-progress"
    :class="mode === 'determinate' ? 'is-small' : 'is-small is-indeterminate'"
    :value="mode === 'determinate' ? value : undefined"
    :max="100"
    :aria-label="label"
  ></progress>
</template>

<script setup lang="ts">
/**
 * 统一的进度条。
 *
 * 上传与下载此前各写一份（上传用 `is-primary`、下载用 `is-info`），颜色与尺寸
 * 不一致；这里统一走设计令牌：轨道 `--vf-surface-sunken`、填充 `--vf-accent`，
 * 未知总量时使用不确定态动画。百分比文案由调用方就近展示，这里不重复渲染
 * （`<progress>` 的 value/max 与 aria-label 已足够表达进度）。
 */
withDefaults(
  defineProps<{
    mode: "determinate" | "indeterminate";
    value?: number;
    /** 供屏幕阅读器使用的描述，例如「上传 main.ts」。 */
    label?: string;
  }>(),
  {
    value: 0,
    label: "进度",
  },
);
</script>

<style scoped>
.vf-progress {
  --bulma-progress-bar-background-color: var(--vf-surface-sunken);
  --bulma-progress-value-background-color: var(--vf-accent);
  height: 0.5rem;
  margin-bottom: 0;
}
</style>
