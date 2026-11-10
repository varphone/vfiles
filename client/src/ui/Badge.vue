<template>
  <span class="vf-badge" :class="[`is-${tone}`, size === 'sm' ? 'is-sm' : '']">
    <span v-if="dot" class="vf-badge-dot" aria-hidden="true"></span>
    <slot />
  </span>
</template>

<script setup lang="ts">
withDefaults(
  defineProps<{
    tone?: "neutral" | "primary" | "success" | "warning" | "danger" | "muted";
    size?: "sm" | "md";
    dot?: boolean;
  }>(),
  { tone: "neutral", size: "md", dot: false },
);
</script>

<style scoped>
.vf-badge {
  display: inline-flex;
  align-items: center;
  gap: var(--vf-badge-gap);
  padding: var(--vf-badge-padding-y) var(--vf-badge-padding-x);
  border: 1px solid transparent;
  border-radius: var(--vf-radius-pill);
  font-size: var(--vf-badge-font-size);
  font-weight: 600;
  line-height: 1.35;
  white-space: nowrap;
}

.vf-badge.is-sm {
  padding: var(--vf-badge-sm-padding-y) var(--vf-badge-sm-padding-x);
  font-size: var(--vf-badge-sm-font-size);
}

.vf-badge-dot {
  width: 6px;
  height: 6px;
  border-radius: 50%;
  background: currentColor;
}

.vf-badge.is-neutral {
  color: var(--vf-text-muted);
  background: var(--vf-surface-2);
  border-color: var(--vf-border-weak);
}

.vf-badge.is-muted {
  color: var(--vf-text-subtle);
  background: var(--vf-surface-2);
}

.vf-badge.is-primary {
  color: var(--vf-accent-text);
  background: var(--vf-accent-soft);
  border-color: var(--vf-accent-line);
}

.vf-badge.is-success {
  color: var(--vf-success-text);
  background: var(--vf-success-soft);
  border-color: var(--vf-success-line);
}

.vf-badge.is-warning {
  color: var(--vf-warning-text);
  background: var(--vf-warning-soft);
  border-color: var(--vf-warning-line);
}

.vf-badge.is-danger {
  color: var(--vf-danger-text);
  background: var(--vf-danger-soft);
  border-color: var(--vf-danger-soft);
}
</style>
