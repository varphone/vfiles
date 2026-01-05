<template>
  <div class="field has-addons">
    <div class="control is-expanded">
      <input
        class="input"
        type="text"
        :value="modelValue"
        :placeholder="placeholder"
        :disabled="disabled"
        @input="onInput"
        @keyup.enter="emit('search')"
      />
    </div>
    <div class="control">
      <button
        class="button is-link"
        type="button"
        :class="{ 'is-loading': loading }"
        :disabled="disabled"
        @click="emit('search')"
      >
        <slot name="search">搜索</slot>
      </button>
    </div>
    <div class="control" v-if="showClear">
      <button
        class="button"
        type="button"
        :disabled="disabled"
        @click="emit('clear')"
      >
        清空
      </button>
    </div>
  </div>
</template>

<script setup lang="ts">
const props = withDefaults(
  defineProps<{
    modelValue: string;
    placeholder?: string;
    loading?: boolean;
    disabled?: boolean;
    showClear?: boolean;
  }>(),
  {
    placeholder: "搜索...",
    loading: false,
    disabled: false,
    showClear: true,
  },
);

const emit = defineEmits<{
  (e: "update:modelValue", v: string): void;
  (e: "search"): void;
  (e: "clear"): void;
}>();

function onInput(e: Event) {
  const target = e.target as HTMLInputElement;
  emit("update:modelValue", target.value);
}
</script>
