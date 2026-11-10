<template>
  <header class="vf-page-header">
    <div class="vf-page-header-main">
      <slot name="breadcrumbs" />
      <div class="vf-page-header-titles">
        <h1 class="vf-page-header-title">
          <slot name="title">{{ title }}</slot>
        </h1>
        <p v-if="description || $slots.description" class="vf-page-header-desc">
          <slot name="description">{{ description }}</slot>
        </p>
      </div>
    </div>
    <div v-if="$slots.actions" class="vf-page-header-actions">
      <slot name="actions" />
    </div>
  </header>
</template>

<script setup lang="ts">
withDefaults(
  defineProps<{
    title?: string;
    description?: string;
  }>(),
  { title: "", description: "" },
);
</script>

<style scoped>
.vf-page-header {
  display: flex;
  align-items: flex-start;
  justify-content: space-between;
  gap: var(--vf-space-4);
  flex-wrap: wrap;
  padding-bottom: var(--vf-space-4);
  border-bottom: 1px solid var(--vf-border-weak);
}

.vf-page-header-main {
  min-width: 0;
}

.vf-page-header-titles {
  min-width: 0;
}

.vf-page-header-title {
  margin: 0;
  font-size: 1.25rem;
  font-weight: 700;
  line-height: 1.3;
  color: var(--vf-text);
  letter-spacing: -0.01em;
}

.vf-page-header-desc {
  margin: var(--vf-space-1) 0 0;
  max-width: 62ch;
  font-size: 0.8125rem;
  line-height: 1.5;
  color: var(--vf-text-muted);
}

.vf-page-header-actions {
  display: flex;
  align-items: center;
  gap: var(--vf-space-2);
  flex-wrap: wrap;
}

@media screen and (max-width: 768px) {
  .vf-page-header-title {
    font-size: 1.1rem;
  }
}
</style>
