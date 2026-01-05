<template>
  <button
    class="button"
    :class="btnClass"
    type="button"
    :disabled="disabled || loading"
    :title="title"
    @click="emit('click')"
  >
    <span v-if="loading" class="loader is-size-7 mr-2"></span>
    <slot />
  </button>
</template>

<script setup lang="ts">
import { computed } from "vue";

const props = withDefaults(
  defineProps<{
    color?:
      | "primary"
      | "link"
      | "info"
      | "success"
      | "warning"
      | "danger"
      | "light"
      | "dark"
      | string;
    light?: boolean;
    small?: boolean;
    loading?: boolean;
    disabled?: boolean;
    title?: string;
  }>(),
  {
    color: "light",
    light: false,
    small: false,
    loading: false,
    disabled: false,
    title: "",
  },
);

const emit = defineEmits<{
  (e: "click"): void;
}>();

const btnClass = computed(() => {
  const classes: Record<string, boolean> = {};
  if (props.color) classes[`is-${props.color}`] = true;
  if (props.light) classes["is-light"] = true;
  if (props.small) classes["is-small"] = true;
  if (props.loading) classes["is-loading"] = true;
  return classes;
});
</script>
