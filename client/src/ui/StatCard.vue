<template>
  <article class="vf-stat">
    <div v-if="icon" class="vf-stat-icon" aria-hidden="true">
      <component :is="icon" :size="18" />
    </div>
    <div class="vf-stat-body">
      <p class="vf-stat-label">{{ label }}</p>
      <p class="vf-stat-value">{{ value }}</p>
      <p v-if="hint" class="vf-stat-hint">{{ hint }}</p>
    </div>
  </article>
</template>

<script setup lang="ts">
import type { Component } from "vue";

withDefaults(
  defineProps<{
    label: string;
    value: string | number;
    hint?: string;
    icon?: Component;
  }>(),
  { hint: "", icon: undefined },
);
</script>

<style scoped>
.vf-stat {
  display: flex;
  align-items: flex-start;
  gap: var(--vf-space-3);
  padding: var(--vf-space-4);
  border: 1px solid var(--vf-border-weak);
  border-radius: var(--vf-radius-card);
  background: var(--vf-surface);
  box-shadow: var(--vf-shadow-card);
}

.vf-stat-icon {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: var(--vf-control-height);
  height: var(--vf-control-height);
  flex: none;
  border-radius: var(--vf-radius-card);
  background: var(--vf-accent-soft);
  color: var(--vf-accent-text);
}

.vf-stat-body {
  min-width: 0;
}

.vf-stat-label {
  margin: 0;
  font-size: 0.76rem;
  font-weight: 550;
  color: var(--vf-text-muted);
}

.vf-stat-value {
  margin: var(--vf-space-1) 0 0;
  font-size: 1.35rem;
  line-height: 1.25;
  font-weight: 700;
  color: var(--vf-text-strong);
  font-variant-numeric: tabular-nums;
}

.vf-stat-hint {
  margin: var(--vf-space-1) 0 0;
  font-size: 0.75rem;
  color: var(--vf-text-subtle);
}
</style>
