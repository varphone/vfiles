<template>
  <div ref="rootRef" class="desktop-search-box">
    <div class="desktop-search-inline">
      <div class="control desktop-search-field">
        <input
          :ref="registerInputElement"
          :value="modelValue"
          class="input is-small desktop-search-control"
          type="text"
          :placeholder="placeholder"
          list="vfiles-search-history-desktop"
          @input="
            emit('update:modelValue', ($event.target as HTMLInputElement).value)
          "
          @keyup.enter="emit('search')"
        />
        <datalist id="vfiles-search-history-desktop">
          <option v-for="item in history" :key="item" :value="item" />
        </datalist>
      </div>

      <div class="desktop-search-action-group">
        <button
          class="vf-ghost-button desktop-search-button"
          :class="{ 'is-active': active }"
          :disabled="loading"
          @click="emit('search')"
        >
          <IconSearch :size="16" />
          <span>搜索</span>
        </button>
        <button
          class="vf-icon-button desktop-search-toggle"
          :class="{
            'is-active': open || filtersActive,
            'is-open': open,
          }"
          title="高级搜索"
          aria-label="高级搜索"
          :aria-expanded="open ? 'true' : 'false'"
          @click="emit('update:open', !open)"
        >
          <IconChevronDown :size="16" />
        </button>
      </div>
    </div>

    <div
      v-if="open"
      class="desktop-search-panel desktop-search-panel--dropdown"
    >
      <div class="desktop-search-panel-heading">高级搜索</div>

      <div class="desktop-search-filters">
        <label class="checkbox desktop-filter-pill">
          <input
            type="checkbox"
            :checked="content"
            :disabled="loading || !contentEnabled"
            @change="
              emit(
                'update:content',
                ($event.target as HTMLInputElement).checked,
              )
            "
          />
          全文搜索
        </label>
        <p v-if="!contentEnabled" class="is-size-7 has-text-warning ml-2">
          (未启用)
        </p>

        <div class="select is-small desktop-filter-select">
          <select
            :value="type"
            :disabled="loading"
            aria-label="搜索类型"
            @change="
              emit(
                'update:type',
                ($event.target as HTMLSelectElement).value as SearchType,
              )
            "
          >
            <option value="all">全部</option>
            <option value="file">仅文件</option>
            <option value="directory">仅文件夹</option>
          </select>
        </div>

        <label class="checkbox desktop-filter-pill">
          <input
            type="checkbox"
            :checked="scopeCurrent"
            :disabled="loading"
            @change="
              emit(
                'update:scopeCurrent',
                ($event.target as HTMLInputElement).checked,
              )
            "
          />
          仅当前目录
        </label>
      </div>

      <div class="desktop-search-dropdown-actions">
        <button
          class="vf-ghost-button desktop-search-clear"
          :disabled="loading"
          @click="emit('clear')"
        >
          清空搜索
        </button>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref } from "vue";
import { IconChevronDown, IconSearch } from "@tabler/icons-vue";

export type SearchType = "all" | "file" | "directory";

/**
 * 桌面端搜索框 + 高级筛选下拉。
 *
 * 搜索状态仍由 `useFileSearch` 持有，这里通过 v-model 双向绑定展示与输入；
 * 下拉的「点击外部关闭」随组件一起迁移，Escape 顺序仍由父组件统一处理。
 */
const props = defineProps<{
  modelValue: string;
  open: boolean;
  content: boolean;
  type: SearchType;
  scopeCurrent: boolean;
  loading: boolean;
  active: boolean;
  contentEnabled: boolean;
  filtersActive: boolean;
  history: string[];
  /** 供父级把输入框登记到搜索组合式函数（清空/搜索后重新聚焦）。 */
  registerInput?: (element: HTMLInputElement | null) => void;
}>();

const emit = defineEmits<{
  (e: "update:modelValue", value: string): void;
  (e: "update:open", value: boolean): void;
  (e: "update:content", value: boolean): void;
  (e: "update:type", value: SearchType): void;
  (e: "update:scopeCurrent", value: boolean): void;
  (e: "search"): void;
  (e: "clear"): void;
}>();

const rootRef = ref<HTMLElement | null>(null);

const placeholder = computed(() =>
  props.content ? "搜索当前工作区中的文本内容" : "搜索名称、扩展名或路径",
);

function registerInputElement(element: unknown) {
  props.registerInput?.(element instanceof HTMLInputElement ? element : null);
}

function onDocumentClick(event: MouseEvent) {
  if (!props.open) return;
  const root = rootRef.value;
  if (root && event.target instanceof Node && !root.contains(event.target)) {
    emit("update:open", false);
  }
}

onMounted(() => {
  document.addEventListener("click", onDocumentClick, true);
});

onBeforeUnmount(() => {
  document.removeEventListener("click", onDocumentClick, true);
  props.registerInput?.(null);
});
</script>

<style scoped>
/* 搜索框占据剩余宽度但不过分拉伸，右侧留给主操作按钮 */
.desktop-search-box {
  position: relative;
  z-index: 3;
  margin-left: auto;
  flex: 1 1 20rem;
  min-width: 8rem;
  /* 主流网盘把搜索做成页面里最大的控件：上限 480px（Drive/Dropbox 量级） */
  max-width: 30rem;
}

.desktop-search-inline {
  display: flex;
  align-items: stretch;
  gap: 0.45rem;
}

.desktop-search-field {
  flex: 1 1 auto;
  min-width: 0;
}

/* 搜索输入做成主流云盘的胶囊搜索框：40px 高于 36px 按钮（搜索是首要控件） */
.desktop-search-control {
  min-height: 2.5rem;
  font-size: 0.875rem;
  border-radius: var(--vf-radius-pill);
  border-color: transparent;
  background: var(--vf-surface-sunken);
  box-shadow: none;
}

.desktop-search-control:focus,
.desktop-search-control:focus-within {
  border-color: var(--vf-accent);
  box-shadow: 0 0 0 3px var(--vf-focus-ring);
  background: var(--vf-surface);
}

.desktop-search-action-group {
  display: inline-flex;
  align-items: stretch;
  flex-shrink: 0;
}

.desktop-search-button {
  justify-content: center;
  border-top-right-radius: 0;
  border-bottom-right-radius: 0;
}

.desktop-search-toggle {
  padding-inline: 0.68rem;
  border-top-left-radius: 0;
  border-bottom-left-radius: 0;
  width: auto;
  /* 覆盖 .vf-icon-button 的固定高度：随行拉伸到 40px，与左侧「搜索」按钮对齐 */
  height: auto;
  align-self: stretch;
  border-radius: 0 var(--vf-radius-sm) var(--vf-radius-sm) 0;
}

.desktop-search-toggle svg {
  transition: transform 0.18s ease;
}

.desktop-search-toggle.is-open svg {
  transform: rotate(180deg);
}

.desktop-search-panel {
  display: flex;
  flex-direction: column;
  gap: 0.65rem;
  /* 与工具栏其它弹层（视图/排序/上传）同一框体语言：
     border-weak、6px 圆角、12px 内边距、raised 背景、菜单阴影 */
  padding: 0.75rem;
  border-radius: 6px;
  border: 1px solid var(--vf-border-weak);
  background: var(--vf-surface-raised);
  box-shadow: var(--vf-shadow-menu);
}

.desktop-search-panel--dropdown {
  position: absolute;
  top: calc(100% + 0.45rem);
  left: auto;
  right: 0;
  width: max-content;
  max-width: min(calc(100vw - 2rem), 28rem);
  z-index: 25;
  opacity: 1;
  isolation: isolate;
  /* 背景/阴影继承 .desktop-search-panel（raised + 菜单阴影，与其它弹层一致） */
}

.desktop-search-panel--dropdown::after {
  content: "";
  position: absolute;
  right: 1.25rem;
  top: -0.42rem;
  width: 0.82rem;
  height: 0.82rem;
  background: var(--vf-surface);
  border-left: 1px solid var(--explorer-panel-border, var(--vf-border));
  border-top: 1px solid var(--explorer-panel-border, var(--vf-border));
  transform: rotate(45deg);
}

.desktop-search-panel-heading {
  font-size: 0.7rem;
  font-weight: 700;
  letter-spacing: 0.08em;
  text-transform: uppercase;
  color: var(--vf-text-muted);
}

.desktop-search-filters {
  display: flex;
  align-items: center;
  gap: 0.55rem;
  flex-wrap: wrap;
  color: var(--vf-text-muted);
  font-size: 0.8rem;
}

.desktop-filter-pill {
  display: inline-flex;
  align-items: center;
  gap: 0.42rem;
  padding: 0.38rem 0.72rem;
  border-radius: var(--vf-radius-pill);
  border: 1px solid var(--vf-border);
  background: var(--vf-surface-sunken);
  line-height: 1;
}

.desktop-filter-pill input {
  margin: 0;
}

.desktop-filter-select select {
  border-radius: var(--vf-radius-pill);
  background-color: var(--vf-surface);
}

.desktop-search-dropdown-actions {
  display: flex;
  justify-content: flex-end;
}

.desktop-search-clear {
  min-width: 0;
}
</style>
