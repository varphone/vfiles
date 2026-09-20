<template>
  <div
    ref="rootRef"
    class="view-options dropdown is-right"
    :class="{ 'is-active': open }"
  >
    <div class="dropdown-trigger">
      <button
        class="button is-small is-light desktop-command-button"
        type="button"
        aria-haspopup="true"
        :aria-expanded="open ? 'true' : 'false'"
        title="视图与排序"
        @click="toggle"
      >
        <IconAdjustmentsHorizontal :size="16" />
        <span>视图</span>
      </button>
    </div>

    <div class="dropdown-menu" role="menu">
      <div class="dropdown-content view-options-panel">
        <div class="view-options-section">
          <p class="view-options-label">显示方式</p>
          <div class="buttons has-addons are-small mb-0">
            <button
              class="button"
              :class="{ 'is-link is-light': view.mode === 'list' }"
              type="button"
              title="列表视图"
              @click="view.setMode('list')"
            >
              <IconList :size="16" />
              <span>列表</span>
            </button>
            <button
              class="button"
              :class="{ 'is-link is-light': view.mode === 'grid' }"
              type="button"
              title="网格视图"
              @click="view.setMode('grid')"
            >
              <IconLayoutGrid :size="16" />
              <span>网格</span>
            </button>
          </div>
        </div>

        <hr class="dropdown-divider" />

        <div class="view-options-section">
          <p class="view-options-label">排序方式</p>
          <div class="field has-addons mb-0">
            <div class="control is-expanded">
              <div class="select is-small is-fullwidth">
                <select
                  :value="view.sortField"
                  aria-label="排序字段"
                  @change="onFieldChange"
                >
                  <option v-for="field in fields" :key="field" :value="field">
                    {{ fieldLabels[field] }}
                  </option>
                </select>
              </div>
            </div>
            <div class="control">
              <button
                class="button is-small is-light"
                type="button"
                :title="view.sortDirection === 'asc' ? '升序' : '降序'"
                @click="view.toggleSortDirection()"
              >
                <IconArrowUp v-if="view.sortDirection === 'asc'" :size="16" />
                <IconArrowDown v-else :size="16" />
              </button>
            </div>
          </div>
        </div>

        <label class="checkbox view-options-checkbox">
          <input
            type="checkbox"
            :checked="view.foldersFirst"
            @change="view.toggleFoldersFirst()"
          />
          文件夹置顶
        </label>

        <template v-if="view.mode === 'grid'">
          <hr class="dropdown-divider" />
          <div class="view-options-section">
            <p class="view-options-label">缩略图大小</p>
            <input
              class="slider is-fullwidth is-small"
              type="range"
              min="96"
              max="240"
              step="8"
              :value="view.thumbnailSize"
              aria-label="缩略图大小"
              @input="onThumbnailSizeInput"
            />
          </div>
        </template>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref } from "vue";
import {
  IconAdjustmentsHorizontal,
  IconArrowDown,
  IconArrowUp,
  IconLayoutGrid,
  IconList,
} from "@tabler/icons-vue";
import { useFileViewStore } from "../../stores/fileView.store";
import { SORT_FIELD_LABELS, type SortField } from "../../utils/fileSort";

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

function toggle() {
  open.value = !open.value;
}

function onFieldChange(event: Event) {
  const target = event.target as HTMLSelectElement;
  view.setSortField(target.value as SortField);
}

function onThumbnailSizeInput(event: Event) {
  const target = event.target as HTMLInputElement;
  view.setThumbnailSize(Number(target.value));
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
.view-options-panel {
  min-width: 230px;
  padding: 12px;
}

.view-options-section {
  margin-bottom: 8px;
}

.view-options-label {
  margin-bottom: 6px;
  font-size: 0.72rem;
  color: #7a7a7a;
  text-transform: uppercase;
  letter-spacing: 0.03em;
}

.view-options-checkbox {
  font-size: 0.8rem;
}

.dropdown-divider {
  margin: 8px 0;
}
</style>
