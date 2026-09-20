<template>
  <div
    ref="rootRef"
    class="sort-menu dropdown is-right"
    :class="{ 'is-active': open }"
  >
    <div class="dropdown-trigger sort-menu-trigger">
      <button
        class="vf-ghost-button"
        :class="{ 'is-active': open }"
        type="button"
        aria-haspopup="true"
        :aria-expanded="open ? 'true' : 'false'"
        :title="`排序：${fieldLabels[view.sortField]}${
          view.sortDirection === 'asc' ? ' 升序' : ' 降序'
        }`"
        @click="toggle"
      >
        <IconArrowsSort :size="16" />
        <span class="is-hidden-touch">{{ fieldLabels[view.sortField] }}</span>
      </button>
      <button
        class="vf-icon-button sort-menu-direction is-hidden-touch"
        type="button"
        :title="
          view.sortDirection === 'asc'
            ? '当前升序，点击改为降序'
            : '当前降序，点击改为升序'
        "
        :aria-label="view.sortDirection === 'asc' ? '升序' : '降序'"
        @click="view.toggleSortDirection()"
      >
        <IconArrowUp v-if="view.sortDirection === 'asc'" :size="16" />
        <IconArrowDown v-else :size="16" />
      </button>
    </div>

    <div class="dropdown-menu" role="menu">
      <div class="dropdown-content sort-menu-panel">
        <p class="sort-menu-label">排序方式</p>
        <button
          v-for="field in fields"
          :key="field"
          class="dropdown-item sort-menu-item"
          :class="{ 'is-active': view.sortField === field }"
          type="button"
          role="menuitemradio"
          :aria-checked="view.sortField === field ? 'true' : 'false'"
          @click="chooseField(field)"
        >
          <IconCheck
            v-if="view.sortField === field"
            :size="16"
            class="mr-2"
            aria-hidden="true"
          />
          <span :class="{ 'ml-4': view.sortField !== field }">{{
            fieldLabels[field]
          }}</span>
        </button>

        <hr class="dropdown-divider" />

        <button
          v-for="option in directions"
          :key="option.value"
          class="dropdown-item sort-menu-item"
          type="button"
          role="menuitemradio"
          :aria-checked="view.sortDirection === option.value ? 'true' : 'false'"
          @click="view.setSortDirection(option.value)"
        >
          <IconCheck
            v-if="view.sortDirection === option.value"
            :size="16"
            class="mr-2"
            aria-hidden="true"
          />
          <span :class="{ 'ml-4': view.sortDirection !== option.value }">{{
            option.label
          }}</span>
        </button>

        <hr class="dropdown-divider" />

        <button
          class="dropdown-item sort-menu-item"
          type="button"
          role="menuitemcheckbox"
          :aria-checked="view.foldersFirst ? 'true' : 'false'"
          @click="view.toggleFoldersFirst()"
        >
          <IconCheck
            v-if="view.foldersFirst"
            :size="16"
            class="mr-2"
            aria-hidden="true"
          />
          <span :class="{ 'ml-4': !view.foldersFirst }">文件夹置顶</span>
        </button>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref } from "vue";
import {
  IconArrowDown,
  IconArrowUp,
  IconArrowsSort,
  IconCheck,
} from "@tabler/icons-vue";
import { useFileViewStore } from "../../stores/fileView.store";
import { SORT_FIELD_LABELS, type SortField } from "../../utils/fileSort";

/**
 * 排序入口。
 *
 * 列表表头可点排序，网格视图此前只能进「视图」下拉里改排序；这里把排序提到工具栏
 * 上一个独立控件（字段菜单 + 方向切换），两种视图共用，状态仍存在 `fileView`。
 */
const view = useFileViewStore();
const open = ref(false);
const rootRef = ref<HTMLElement | null>(null);

const fields = computed<SortField[]>(() => [
  "name",
  "modified",
  "size",
  "type",
]);
const fieldLabels = SORT_FIELD_LABELS;
// 移动端搜索行放不下独立的方向按钮，菜单里也提供一份
const directions: { value: "asc" | "desc"; label: string }[] = [
  { value: "asc", label: "升序" },
  { value: "desc", label: "降序" },
];

function toggle() {
  open.value = !open.value;
}

function chooseField(field: SortField) {
  if (view.sortField === field) {
    // 重复点击同一字段时切换升降序，与表头行为一致
    view.toggleSortDirection();
  } else {
    view.setSortField(field);
  }
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
.sort-menu-trigger {
  display: inline-flex;
  align-items: center;
  gap: 0.1rem;
}

.sort-menu-panel {
  min-width: 170px;
  padding: 6px;
  background: var(--vf-surface-raised);
  border: 1px solid var(--vf-border);
  box-shadow: var(--vf-shadow-menu);
}

.sort-menu-label {
  padding: 4px 8px;
  font-size: 0.72rem;
  color: var(--vf-text-muted);
  text-transform: uppercase;
  letter-spacing: 0.03em;
}

.sort-menu-item {
  display: flex;
  align-items: center;
  border-radius: var(--vf-radius-sm);
  color: var(--vf-text);
  background: transparent;
}

.sort-menu-item:hover,
.sort-menu-item:focus-visible {
  background: var(--vf-surface-hover);
  color: var(--vf-text-strong);
}

.sort-menu-item.is-active {
  color: var(--vf-accent-strong);
  font-weight: 600;
}
</style>
