<template>
  <div v-if="visible" class="notification is-danger is-light">
    <slot>{{ message }}</slot>
  </div>
</template>

<script setup lang="ts">
import { computed } from 'vue';

const props = defineProps<{
  message?: string | null;
}>();

const visible = computed(() => {
  const m = (props.message || '').trim();
  return m.length > 0;
});
</script>
