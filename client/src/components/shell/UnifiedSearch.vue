<template>
  <Modal :show="show" title="搜索" mobile-compact @close="emit('close')">
    <div class="unified-search">
      <div class="unified-search-field">
        <IconSearch :size="17" class="unified-search-icon" aria-hidden="true" />
        <input
          ref="inputRef"
          v-model="query"
          class="input unified-search-input"
          type="search"
          placeholder="搜索文件与文件夹…"
          aria-label="搜索文件"
          autocomplete="off"
          data-autofocus
        />
        <span v-if="loading" class="unified-search-loading" aria-hidden="true">
          <span class="unified-search-spinner"></span>
        </span>
      </div>

      <p v-if="error" class="unified-search-hint is-error">{{ error }}</p>

      <p v-else-if="!trimmedQuery" class="unified-search-hint">
        输入文件名后自动搜索，Esc 关闭。
      </p>

      <p
        v-else-if="loading && results.length === 0"
        class="unified-search-hint"
      >
        搜索中…
      </p>

      <p v-else-if="results.length === 0" class="unified-search-hint">
        没有找到匹配的文件
      </p>

      <div v-else class="unified-search-results">
        <section
          v-for="group in resultGroups"
          :key="group.key"
          class="unified-search-group"
          :aria-label="group.label"
        >
          <p class="unified-search-group-title">{{ group.label }}</p>
          <ul class="unified-search-list">
            <li v-for="item in group.items" :key="item.path">
              <button
                class="unified-search-result"
                type="button"
                :title="item.path"
                @click="openResult(item)"
              >
                <FileTypeIcon :file="item" :size="18" />
                <span class="unified-search-text">
                  <span class="unified-search-name">{{ item.name }}</span>
                  <span class="unified-search-path">{{ item.path }}</span>
                </span>
              </button>
            </li>
          </ul>
        </section>
      </div>
    </div>
  </Modal>
</template>

<script setup lang="ts">
import { computed, nextTick, onBeforeUnmount, ref, watch } from "vue";
import { useRouter } from "vue-router";
import { IconSearch } from "@tabler/icons-vue";
import Modal from "../common/Modal.vue";
import FileTypeIcon from "../file-browser/FileTypeIcon.vue";
import { filesService } from "../../services/files.service";
import { parentDirectoryPath } from "../../utils/filePaths";
import type { FileInfo } from "../../types";

/**
 * 统一搜索：顶栏搜索按钮与 Ctrl/Cmd+K 打开的全局搜索。
 *
 * 输入 300ms 防抖后调用 `filesService.searchFiles`，结果按 Nextcloud 的
 * 「分组 + 图标 + 名称 + 路径」结构展示；点击结果跳到根目录并带上 `path`，
 * 由文件列表在下一阶段消费该参数并高亮目标位置。
 */
const props = defineProps<{
  show: boolean;
}>();

const emit = defineEmits<{
  close: [];
}>();

/** 防抖时长：与主流网盘的即时搜索节奏一致。 */
const SEARCH_DEBOUNCE_MS = 300;
/** 模态内只展示首屏结果，完整搜索仍在文件列表里进行。 */
const SEARCH_RESULT_LIMIT = 20;

const router = useRouter();
const query = ref("");
const results = ref<FileInfo[]>([]);
const loading = ref(false);
const error = ref<string | null>(null);
const inputRef = ref<HTMLInputElement | null>(null);

let debounceTimer: ReturnType<typeof setTimeout> | null = null;
// 请求序号：关闭或再次输入时丢弃在途结果，避免旧结果覆盖新查询。
let searchSequence = 0;

const trimmedQuery = computed(() => query.value.trim());

const resultGroups = computed(() => {
  if (results.value.length === 0) return [];
  return [{ key: "files", label: "文件", items: results.value }];
});

function clearDebounce() {
  if (debounceTimer === null) return;
  clearTimeout(debounceTimer);
  debounceTimer = null;
}

function resetSearch() {
  clearDebounce();
  searchSequence += 1;
  query.value = "";
  results.value = [];
  error.value = null;
  loading.value = false;
}

async function runSearch(term: string) {
  const requestId = ++searchSequence;
  loading.value = true;
  error.value = null;

  try {
    const page = await filesService.searchFiles(term, "name", {
      limit: SEARCH_RESULT_LIMIT,
    });
    if (requestId !== searchSequence) return;
    results.value = page.items;
  } catch (err) {
    if (requestId !== searchSequence) return;
    error.value = err instanceof Error ? err.message : "搜索失败";
    results.value = [];
  } finally {
    if (requestId === searchSequence) loading.value = false;
  }
}

watch(query, (value) => {
  clearDebounce();
  const term = value.trim();

  if (!term) {
    // 清空输入时立刻收掉结果，不留上一轮的内容。
    searchSequence += 1;
    results.value = [];
    error.value = null;
    loading.value = false;
    return;
  }

  debounceTimer = setTimeout(() => {
    debounceTimer = null;
    void runSearch(term);
  }, SEARCH_DEBOUNCE_MS);
});

watch(
  () => props.show,
  (visible) => {
    if (!visible) {
      resetSearch();
      return;
    }
    void nextTick().then(() => inputRef.value?.focus());
  },
);

/** 点击结果：关闭模态并跳到文件根目录，目录参数由结果路径推导。 */
function openResult(item: FileInfo) {
  const directory =
    item.kind === "directory" ? item.path : parentDirectoryPath(item.path);
  emit("close");
  void router.push({ path: "/", query: { path: directory } });
}

onBeforeUnmount(() => {
  clearDebounce();
  searchSequence += 1;
});
</script>

<style scoped>
.unified-search {
  display: flex;
  flex-direction: column;
  gap: 0.6rem;
  min-width: 0;
}

.unified-search-field {
  position: relative;
  display: flex;
  align-items: center;
}

.unified-search-icon {
  position: absolute;
  left: 0.6rem;
  color: var(--vf-text-subtle);
  pointer-events: none;
}

.unified-search-input {
  width: 100%;
  padding-left: 2.1rem;
  padding-right: var(--vf-space-6);
}

.unified-search-loading {
  position: absolute;
  right: 0.65rem;
  display: inline-flex;
  align-items: center;
  justify-content: center;
}

.unified-search-spinner {
  width: 0.9rem;
  height: 0.9rem;
  border: 2px solid var(--vf-border);
  border-top-color: var(--vf-accent);
  border-radius: 50%;
  animation: unified-search-spin 0.7s linear infinite;
}

@keyframes unified-search-spin {
  to {
    transform: rotate(360deg);
  }
}

.unified-search-hint {
  margin: 0;
  padding: 1.1rem var(--vf-space-1);
  color: var(--vf-text-muted);
  font-size: var(--vf-text-sm);
  text-align: center;
}

.unified-search-hint.is-error {
  color: var(--vf-danger-text);
}

.unified-search-results {
  max-height: min(56vh, 26rem);
  overflow-y: auto;
  -webkit-overflow-scrolling: touch;
}

.unified-search-group + .unified-search-group {
  margin-top: var(--vf-space-2);
}

.unified-search-group-title {
  margin: 0;
  padding: 0.3rem 0.4rem;
  color: var(--vf-text-muted);
  font-size: var(--vf-text-xs);
  font-weight: 600;
  letter-spacing: 0.02em;
}

.unified-search-list {
  display: flex;
  flex-direction: column;
  gap: 1px;
  margin: 0;
  padding: 0;
  list-style: none;
}

/* 结果行：40px 高、图标 + 名称 + 路径，悬停用中性灰。 */
.unified-search-result {
  display: flex;
  align-items: center;
  gap: 0.6rem;
  width: 100%;
  min-height: 40px;
  padding: 0.3rem 0.4rem;
  border: none;
  border-radius: 3px;
  background: transparent;
  color: var(--vf-text);
  text-align: left;
  cursor: pointer;
}

.unified-search-result:hover,
.unified-search-result:focus-visible {
  background: var(--vf-row-hover);
}

.unified-search-text {
  display: flex;
  flex-direction: column;
  min-width: 0;
  gap: 0.05rem;
}

.unified-search-name,
.unified-search-path {
  overflow: hidden;
  white-space: nowrap;
  text-overflow: ellipsis;
}

.unified-search-name {
  font-size: var(--vf-text-base);
}

.unified-search-path {
  color: var(--vf-text-subtle);
  font-size: var(--vf-text-xs);
}

@media (prefers-reduced-motion: reduce) {
  .unified-search-spinner {
    animation-duration: 0.01ms;
    animation-iteration-count: 1;
  }
}
</style>
