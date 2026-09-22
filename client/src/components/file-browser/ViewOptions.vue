<template>
  <div
    ref="rootRef"
    class="view-options dropdown is-right"
    :class="{ 'is-active': open }"
  >
    <div class="dropdown-trigger">
      <button
        class="vf-ghost-button"
        type="button"
        aria-haspopup="true"
        :aria-expanded="open ? 'true' : 'false'"
        title="视图与排序"
        @click="toggle"
      >
        <IconAdjustmentsHorizontal :size="16" />
        <span class="is-hidden-touch">视图</span>
      </button>
    </div>

    <div class="dropdown-menu" role="menu">
      <div class="dropdown-content view-options-panel">
        <div class="view-options-section">
          <p class="view-options-label">显示方式</p>
          <div class="view-options-segment" role="group" aria-label="显示方式">
            <button
              class="vf-ghost-button view-options-segment-button"
              :class="{ 'is-active': view.mode === 'list' }"
              type="button"
              title="列表视图"
              :aria-pressed="view.mode === 'list' ? 'true' : 'false'"
              @click="view.setMode('list')"
            >
              <IconList :size="16" />
              <span>列表</span>
            </button>
            <button
              class="vf-ghost-button view-options-segment-button"
              :class="{ 'is-active': view.mode === 'grid' }"
              type="button"
              title="网格视图"
              :aria-pressed="view.mode === 'grid' ? 'true' : 'false'"
              @click="view.setMode('grid')"
            >
              <IconLayoutGrid :size="16" />
              <span>网格</span>
            </button>
          </div>
        </div>

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
import { onBeforeUnmount, onMounted, ref } from "vue";
import {
  IconAdjustmentsHorizontal,
  IconLayoutGrid,
  IconList,
} from "@tabler/icons-vue";
import { useFileViewStore } from "../../stores/fileView.store";

const view = useFileViewStore();
const open = ref(false);
const rootRef = ref<HTMLElement | null>(null);

function toggle() {
  open.value = !open.value;
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

/* 弹层定位：
   Bulma 的 .is-right 让面板右缘对齐触发器、向左展开；但「视图」按钮在命令栏左侧，
   面板左缘会越过 .file-browser-box（overflow: hidden）被裁掉。
   - 桌面：左缘对齐触发器、向右展开（按钮在命令栏左侧，右侧空间充足）；
   - 窄屏（移动端搜索行）：以 .mobile-search-row 为定位容器、宽度内水平居中，
     无论按钮在行内什么位置都不会被外层裁掉。 */
.view-options.dropdown .dropdown-menu {
  left: 0;
  right: auto;
  transform: none;
}

@media screen and (max-width: 1023px) {
  .mobile-search-row .view-options.dropdown {
    position: static;
  }

  .mobile-search-row .view-options.dropdown .dropdown-menu {
    left: 0;
    right: 0;
    width: fit-content;
    max-width: 100%;
    margin-inline: auto;
  }
}

.view-options-section {
  margin-bottom: 8px;
}

/* 下拉展开时高亮「视图」触发器本身。
   注意只能用直接子选择器：面板也在 .view-options 内部，
   若写成后代选择器会把面板里的显示方式按钮一起点亮，导致激活态看起来不对。 */
.view-options.dropdown.is-active > .dropdown-trigger .vf-ghost-button {
  background: var(--vf-accent-soft);
  color: var(--vf-accent-text);
}

/* 显示方式：分段控件，激活项用强调色浅底 */
.view-options-segment {
  display: inline-flex;
  gap: 0.15rem;
  padding: 0.15rem;
  border-radius: var(--vf-radius);
  background: var(--vf-surface-sunken);
}

.view-options-segment-button {
  min-height: 1.75rem;
  padding: 0 0.6rem;
  font-size: 0.78rem;
}

.view-options-label {
  margin-bottom: 6px;
  font-size: 0.72rem;
  color: var(--vf-text-muted);
  text-transform: uppercase;
  letter-spacing: 0.03em;
}

.dropdown-divider {
  margin: 8px 0;
}
</style>
