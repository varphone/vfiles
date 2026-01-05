<template>
  <progress
    v-if="mode === 'determinate'"
    class="progress is-primary is-small"
    :value="value"
    max="100"
  >
    {{ value }}%
  </progress>
  <progress v-else class="progress is-primary is-small" max="100"></progress>
</template>

<script setup lang="ts">
withDefaults(
  defineProps<{
    mode: "determinate" | "indeterminate";
    value?: number;
  }>(),
  {
    value: 0,
  },
);
</script>
