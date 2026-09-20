<template>
  <div
    ref="rootRef"
    class="theme-toggle dropdown is-right"
    :class="{ 'is-active': open }"
  >
    <div class="dropdown-trigger">
      <button
        class="button is-small is-light theme-toggle-button"
        type="button"
        aria-haspopup="true"
        :aria-expanded="open ? 'true' : 'false'"
        :title="`主题：${currentLabel}（点击切换）`"
        :aria-label="`主题：${currentLabel}`"
        @click="toggle"
      >
        <component :is="currentIcon" :size="16" />
        <span class="theme-toggle-label is-hidden-touch">{{
          currentLabel
        }}</span>
      </button>
    </div>

    <div class="dropdown-menu" role="menu">
      <div class="dropdown-content theme-toggle-panel">
        <p class="theme-toggle-title">外观</p>
        <button
          v-for="option in options"
          :key="option.value"
          class="dropdown-item theme-toggle-item"
          :class="{ 'is-active': theme.mode === option.value }"
          type="button"
          role="menuitemradio"
          :aria-checked="theme.mode === option.value ? 'true' : 'false'"
          @click="choose(option.value)"
        >
          <component :is="option.icon" :size="16" class="mr-2" />
          <span>{{ option.label }}</span>
        </button>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref } from "vue";
import { IconDeviceDesktop, IconMoon, IconSun } from "@tabler/icons-vue";
import { useThemeStore, type ThemeMode } from "../../stores/theme.store";

const theme = useThemeStore();
const open = ref(false);
const rootRef = ref<HTMLElement | null>(null);

const options: { value: ThemeMode; label: string; icon: unknown }[] = [
  { value: "system", label: "跟随系统", icon: IconDeviceDesktop },
  { value: "light", label: "浅色", icon: IconSun },
  { value: "dark", label: "深色", icon: IconMoon },
];

const currentLabel = computed(
  () => options.find((option) => option.value === theme.mode)?.label ?? "主题",
);

const currentIcon = computed(() => {
  if (theme.mode === "light") return IconSun;
  if (theme.mode === "dark") return IconMoon;
  return IconDeviceDesktop;
});

function toggle() {
  open.value = !open.value;
}

function choose(mode: ThemeMode) {
  theme.setMode(mode);
  open.value = false;
}

function onDocumentClick(event: MouseEvent) {
  if (!open.value) return;
  const root = rootRef.value;
  if (root && event.target instanceof Node && !root.contains(event.target)) {
    open.value = false;
  }
}

function onKeydown(event: KeyboardEvent) {
  if (event.key === "Escape") open.value = false;
}

onMounted(() => {
  document.addEventListener("click", onDocumentClick, true);
  document.addEventListener("keydown", onKeydown);
});

onBeforeUnmount(() => {
  document.removeEventListener("click", onDocumentClick, true);
  document.removeEventListener("keydown", onKeydown);
});
</script>

<style scoped>
.theme-toggle-panel {
  min-width: 160px;
  padding: 6px;
  background: var(--vf-surface-raised);
  border: 1px solid var(--vf-border);
  box-shadow: var(--vf-shadow-menu);
}

.theme-toggle-title {
  padding: 4px 8px;
  font-size: 0.72rem;
  color: var(--vf-text-muted);
  text-transform: uppercase;
  letter-spacing: 0.03em;
}

.theme-toggle-item {
  display: flex;
  align-items: center;
  border-radius: 6px;
  color: var(--vf-text);
  background: transparent;
}

.theme-toggle-item:hover,
.theme-toggle-item:focus-visible {
  background: var(--vf-surface-hover);
  color: var(--vf-text-strong);
}

.theme-toggle-item.is-active {
  background: var(--vf-accent-soft);
  color: var(--vf-accent-text);
}
</style>
