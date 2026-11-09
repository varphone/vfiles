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
  border-radius: var(--vf-radius-lg);
  background: var(--vf-surface);
  box-shadow: var(--vf-shadow-card);
  overflow: hidden;
}

.vf-card-head {
  display: flex;
  align-items: flex-start;
  justify-content: space-between;
  gap: 0.75rem;
  flex-wrap: wrap;
  padding: 0.95rem 1.1rem 0.75rem;
}

.vf-card-title {
  margin: 0;
  font-size: 0.95rem;
  font-weight: 650;
  color: var(--vf-text-strong);
}

.vf-card-desc {
  margin: 0.2rem 0 0;
  font-size: 0.78rem;
  line-height: 1.5;
  color: var(--vf-text-muted);
}

.vf-card-actions {
  display: flex;
  align-items: center;
  gap: 0.4rem;
  flex-wrap: wrap;
}

.vf-card-body {
  padding: 0 1.1rem 1.1rem;
}

.vf-card-body.is-flush,
.vf-card.is-flush .vf-card-body {
  padding: 0;
}

.vf-card-foot {
  padding: 0.7rem 1.1rem;
  border-top: 1px solid var(--vf-border-weak);
  background: var(--vf-surface-2);
}

@media screen and (max-width: 768px) {
  .vf-card-head {
    padding: 0.8rem 0.9rem 0.65rem;
  }

  .vf-card-body {
    padding: 0 0.9rem 0.95rem;
  }
}
</style>
