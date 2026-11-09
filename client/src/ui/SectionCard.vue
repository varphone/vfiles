<template>
  <section class="vf-card" :class="{ 'is-flush': flush }">
    <header v-if="title || $slots.actions" class="vf-card-head">
      <div class="vf-card-heading">
        <h2 v-if="title" class="vf-card-title">{{ title }}</h2>
        <p v-if="description" class="vf-card-desc">{{ description }}</p>
      </div>
      <div v-if="$slots.actions" class="vf-card-actions">
        <slot name="actions" />
      </div>
    </header>
    <div class="vf-card-body" :class="{ 'is-flush': flush }">
      <slot />
    </div>
    <footer v-if="$slots.footer" class="vf-card-foot">
      <slot name="footer" />
    </footer>
  </section>
</template>

<script setup lang="ts">
withDefaults(
  defineProps<{
    title?: string;
    description?: string;
    /** 去掉内边距，交给表格/列表自行贴边。 */
    flush?: boolean;
  }>(),
  { title: "", description: "", flush: false },
);
</script>

<style scoped>
.vf-card {
  border: 1px solid var(--vf-border-weak);
  border-radius: var(--vf-radius-card);
  background: var(--vf-surface);
  box-shadow: var(--vf-shadow-card);
  overflow: hidden;
}

.vf-card-head {
  display: flex;
  align-items: flex-start;
  justify-content: space-between;
  gap: var(--vf-space-3);
  flex-wrap: wrap;
  padding: var(--vf-space-4) var(--vf-space-4) var(--vf-space-3);
}

.vf-card-title {
  margin: 0;
  font-size: 0.95rem;
  font-weight: 650;
  color: var(--vf-text-strong);
}

.vf-card-desc {
  margin: var(--vf-space-1) 0 0;
  font-size: 0.8125rem;
  line-height: 1.5;
  color: var(--vf-text-muted);
}

.vf-card-actions {
  display: flex;
  align-items: center;
  gap: var(--vf-space-2);
  flex-wrap: wrap;
}

.vf-card-body {
  padding: 0 var(--vf-space-4) var(--vf-space-4);
}

.vf-card-body.is-flush,
.vf-card.is-flush .vf-card-body {
  padding: 0;
}

.vf-card-foot {
  padding: var(--vf-space-3) var(--vf-space-4);
  border-top: 1px solid var(--vf-border-weak);
  background: var(--vf-surface-2);
}

@media screen and (max-width: 768px) {
  .vf-card-head {
    padding: var(--vf-space-3);
  }

  .vf-card-body {
    padding: 0 var(--vf-space-3) var(--vf-space-3);
  }
}
</style>
