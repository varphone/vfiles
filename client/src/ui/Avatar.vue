<template>
  <span
    class="vf-avatar"
    :class="`is-${size}`"
    :style="src ? undefined : { background: background, color: foreground }"
    :title="name"
  >
    <img v-if="src" :src="src" :alt="name" />
    <span v-else aria-hidden="true">{{ initials }}</span>
  </span>
</template>

<script setup lang="ts">
import { computed } from "vue";

const props = withDefaults(
  defineProps<{
    name: string;
    size?: "xs" | "sm" | "md" | "lg";
    src?: string;
  }>(),
  { size: "md", src: "" },
);

const initials = computed(() => {
  const trimmed = props.name.trim();
  if (!trimmed) return "?";
  const parts = trimmed.split(/\s+/).filter(Boolean);
  if (parts.length >= 2) {
    return (parts[0][0] + parts[1][0]).toUpperCase();
  }
  return Array.from(trimmed)[0].toUpperCase();
});

/** 由用户名派生稳定色相，保证同一用户在各列表颜色一致。 */
const palette = computed(() => {
  let hash = 0;
  for (const ch of props.name) {
    hash = (hash * 31 + ch.codePointAt(0)!) % 360;
  }
  return {
    background: `hsl(${hash}, 62%, 92%)`,
    foreground: `hsl(${hash}, 58%, 30%)`,
  };
});

const background = computed(() => palette.value.background);
const foreground = computed(() => palette.value.foreground);
</script>

<style scoped>
.vf-avatar {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  flex: none;
  overflow: hidden;
  border-radius: 50%;
  font-weight: 700;
  line-height: 1;
  user-select: none;
}

.vf-avatar img {
  width: 100%;
  height: 100%;
  object-fit: cover;
}

.vf-avatar.is-xs {
  width: 22px;
  height: 22px;
  font-size: 0.62rem;
}

.vf-avatar.is-sm {
  width: 28px;
  height: 28px;
  font-size: 0.7rem;
}

.vf-avatar.is-md {
  width: 34px;
  height: 34px;
  font-size: 0.78rem;
}

.vf-avatar.is-lg {
  width: 44px;
  height: 44px;
  font-size: 0.95rem;
}
</style>
