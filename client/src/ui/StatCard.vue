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
  gap: 0.75rem;
  padding: 0.95rem 1rem;
  border: 1px solid var(--vf-border-weak);
  border-radius: var(--vf-radius-lg);
  background: var(--vf-surface);
  box-shadow: var(--vf-shadow-card);
}

.vf-stat-icon {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 34px;
  height: 34px;
  flex: none;
  border-radius: var(--vf-radius-sm);
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
  margin: 0.15rem 0 0;
  font-size: 1.35rem;
  line-height: 1.25;
  font-weight: 700;
  color: var(--vf-text-strong);
  font-variant-numeric: tabular-nums;
}

.vf-stat-hint {
  margin: 0.2rem 0 0;
  font-size: 0.74rem;
  color: var(--vf-text-subtle);
}
</style>
