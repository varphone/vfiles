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
          :aria-label="placeholder"
          autocomplete="off"
          @focus="historyOpen = historyItems.length > 0"
          @input="onInput"
          @keyup.enter="onEnter"
          @keydown.esc.prevent="onEscape"
          @keydown.down.prevent="onArrow(1)"
          @keydown.up.prevent="onArrow(-1)"
        />
      </div>

      <!-- 历史条目：原生 datalist 无法与站内浮层统一样式与定位，改用自绘面板 -->
      <div
        v-if="historyOpen && historyItems.length > 0"
        class="desktop-history-panel"
        role="listbox"
        aria-label="搜索历史"
      >
        <p class="desktop-history-heading">最近搜索</p>
        <button
          v-for="(item, index) in historyItems"
          :key="item"
          class="desktop-history-item"
          :class="{ 'is-highlighted': index === historyIndex }"
          type="button"
          role="option"
          :aria-selected="index === historyIndex ? 'true' : 'false'"
          @mousedown.prevent
          @click="pickHistory(item)"
          @mouseenter="historyIndex = index"
        >
          <IconHistory :size="14" />
          <span>{{ item }}</span>
        </button>
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
import { IconChevronDown, IconHistory, IconSearch } from "@tabler/icons-vue";

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
  /** 搜索框动线（r62）：Esc 清搜索 / ↓ 进入结果首项 */
  (e: "enter-results"): void;
  (e: "update:open", value: boolean): void;
  (e: "update:content", value: boolean): void;
  (e: "update:type", value: SearchType): void;
  (e: "update:scopeCurrent", value: boolean): void;
  (e: "search"): void;
  (e: "clear"): void;
}>();

const rootRef = ref<HTMLElement | null>(null);

/** 搜索历史浮层：聚焦时展开，匹配当前输入；键盘上下选择、回车填入并搜索。 */
const historyOpen = ref(false);
const historyIndex = ref(-1);

const historyItems = computed(() => {
  const keyword = props.modelValue.trim().toLowerCase();
  const items = keyword
    ? props.history.filter((item) => item.toLowerCase().includes(keyword))
    : props.history;
  return items.slice(0, 8);
});

function onInput(event: Event) {
  const value = (event.target as HTMLInputElement).value;
  emit("update:modelValue", value);
  historyOpen.value = true;
  historyIndex.value = -1;
}

function onEnter() {
  if (historyOpen.value && historyIndex.value >= 0) {
    const item = historyItems.value[historyIndex.value];
    if (item) {
      pickHistory(item);
      return;
    }
  }
  historyOpen.value = false;
  emit("search");
}

function onEscape() {
  if (historyOpen.value) {
    historyOpen.value = false;
    return;
  }
  if (props.open) {
    emit("update:open", false);
    return;
  }
  emit("clear");
}

function onArrow(step: number) {
  if (!historyOpen.value && historyItems.value.length > 0) {
    historyOpen.value = true;
    historyIndex.value = -1;
  }
  if (!historyOpen.value || historyItems.value.length === 0) {
    // 没有历史条目时保持原有行为：↓ 直接进入结果首项
    if (step > 0) emit("enter-results");
    return;
  }
  const total = historyItems.value.length;
  historyIndex.value = (historyIndex.value + step + total) % total;
}

function pickHistory(item: string) {
  emit("update:modelValue", item);
  historyOpen.value = false;
  historyIndex.value = -1;
  emit("search");
}

const placeholder = computed(() =>
  props.content ? "搜索当前工作区中的文本内容" : "搜索名称、扩展名或路径",
);

function registerInputElement(element: unknown) {
  props.registerInput?.(element instanceof HTMLInputElement ? element : null);
}

function onDocumentClick(event: MouseEvent) {
  const root = rootRef.value;
  if (!root || !(event.target instanceof Node) || root.contains(event.target)) {
    return;
  }
  historyOpen.value = false;
  if (props.open) emit("update:open", false);
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
  flex: 1 1 16rem;
  min-width: 7rem;
  /* 合并成单行工具栏后搜索框让位给左右操作：上限 26rem，仍比图标按钮显眼 */
  max-width: 26rem;
}

.desktop-search-inline {
  display: flex;
  align-items: stretch;
  gap: var(--vf-space-2);
}

.desktop-search-field {
  flex: 1 1 auto;
  min-width: 0;
}

/* 工具栏右侧的搜索框：白底 + 3px 圆角，34px 高与同行按钮对齐。 */
.desktop-search-control {
  min-height: var(--vf-control-height);
  font-size: var(--vf-text-base);
  border: 1px solid var(--vf-border-weak);
  border-radius: var(--vf-radius-control);
  background: var(--vf-surface);
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
  border-radius: 0 var(--vf-radius-control) var(--vf-radius-control) 0;
}

.desktop-search-toggle svg {
  transition: transform 0.18s var(--vf-motion-standard);
}

.desktop-search-toggle.is-open svg {
  transform: rotate(180deg);
}

/* 搜索历史浮层：与排序/视图/高级搜索同一套框体与定位（输入框左缘、下 6px） */
.desktop-history-panel {
  position: absolute;
  top: calc(100% + 6px);
  left: 0;
  z-index: 31;
  display: flex;
  flex-direction: column;
  gap: 2px;
  width: 100%;
  max-width: 26rem;
  padding: 6px;
  border: 1px solid var(--vf-border-weak);
  border-radius: var(--vf-radius-sm);
  background: var(--vf-surface);
  box-shadow: var(--vf-shadow-popover);
}

.desktop-history-heading {
  margin: 0;
  padding: 4px 8px 6px;
  font-size: var(--vf-text-xs);
  font-weight: 600;
  color: var(--vf-text-subtle);
}

.desktop-history-item {
  display: flex;
  align-items: center;
  gap: 8px;
  width: 100%;
  min-height: 34px;
  padding: 0 8px;
  border: 0;
  border-radius: var(--vf-radius-xs);
  background: transparent;
  color: var(--vf-text);
  font-size: var(--vf-text-sm);
  text-align: left;
  cursor: pointer;
}

.desktop-history-item svg {
  flex: none;
  color: var(--vf-text-subtle);
}

.desktop-history-item:hover,
.desktop-history-item.is-highlighted {
  background: var(--vf-row-hover);
  color: var(--vf-text);
}

.desktop-search-panel {
  display: flex;
  flex-direction: column;
  gap: var(--vf-space-3);
  /* 与 .dropdown-content / 排序菜单同一套框体：白底、1px 细边、8px 圆角、
     12px 内边距、popover 阴影。此前用 raised + menu 阴影并额外画了三角箭头，
     是页面上唯一带箭头的浮层，和其余弹层对不上。 */
  padding: var(--vf-space-3);
  border-radius: var(--vf-radius-sm);
  border: 1px solid var(--vf-border-weak);
  background: var(--vf-surface);
  box-shadow: var(--vf-shadow-popover);
}

.desktop-search-panel--dropdown {
  position: absolute;
  /* 与输入框左缘对齐并向下 6px：right:0 会把面板推到右侧按钮组下方，
     看起来像「脱离了搜索框」 */
  top: calc(100% + 6px);
  left: 0;
  right: auto;
  width: max-content;
  min-width: 100%;
  max-width: min(calc(100vw - 2rem), 28rem);
  z-index: 30;
  opacity: 1;
  isolation: isolate;
}

.desktop-search-panel-heading {
  font-size: var(--vf-text-xs);
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
  font-size: var(--vf-text-sm);
}

.desktop-filter-pill {
  display: inline-flex;
  align-items: center;
  gap: 0.42rem;
  padding: 0.4rem 0.72rem;
  border-radius: var(--vf-radius-pill);
  border: 1px solid var(--vf-border);
  background: var(--vf-surface-2);
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

/* reduced-motion：transform/width/all 过渡含位移或布局动画，降级为瞬时
   （色/透明/阴影类淡入不在此列 = 无位移风险）。 */
@media (prefers-reduced-motion: reduce) {
  *,
  *::before,
  *::after {
    animation-duration: 0.01ms !important;
    animation-iteration-count: 1 !important;
    transition-duration: 0.01ms !important;
  }
}
</style>
