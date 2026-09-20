import { computed, nextTick, ref, type Ref } from "vue";
import { filesService } from "../services/files.service";
import type { FileInfo } from "../types";

const SEARCH_HISTORY_KEY = "vfiles.searchHistory";

/**
 * 文件/内容搜索：查询条件、结果、加载态、桌面高级搜索面板与搜索历史。
 *
 * 从 FileBrowser 抽出；搜索请求带序号，快速连续搜索时只接受最新结果。
 */
export function useFileSearch(currentPath: Ref<string>) {
  const searchQuery = ref("");
  const searchResults = ref<FileInfo[]>([]);
  const searchLoading = ref(false);
  const searchError = ref<string | null>(null);
  const searchActive = ref(false);
  const searchContent = ref(false);
  const searchType = ref<"all" | "file" | "directory">("all");
  const searchScopeCurrent = ref(false);
  const desktopSearchOpen = ref(false);
  const desktopSearchBoxRef = ref<HTMLElement | null>(null);
  const desktopSearchInputRef = ref<HTMLInputElement | null>(null);
  const searchHistory = ref<string[]>([]);

  const searchMode = computed(() => (searchContent.value ? "content" : "name"));
  const desktopSearchFiltersActive = computed(
    () =>
      searchContent.value ||
      searchType.value !== "all" ||
      searchScopeCurrent.value,
  );

  // 搜索请求序号：快速连续搜索时只接受最后一次的结果，避免旧结果覆盖新结果。
  let searchSequence = 0;

  function loadSearchHistory() {
    try {
      const raw = localStorage.getItem(SEARCH_HISTORY_KEY);
      if (!raw) return;
      const parsed = JSON.parse(raw);
      if (Array.isArray(parsed)) {
        searchHistory.value = parsed
          .filter((item) => typeof item === "string")
          .slice(0, 10);
      }
    } catch {
      // ignore
    }
  }

  function saveSearchHistory(next: string[]) {
    searchHistory.value = next;
    try {
      localStorage.setItem(SEARCH_HISTORY_KEY, JSON.stringify(next));
    } catch {
      // ignore
    }
  }

  function pushSearchHistory(term: string) {
    const value = term.trim();
    if (!value) return;
    const withoutDup = searchHistory.value.filter(
      (item) => item.toLowerCase() !== value.toLowerCase(),
    );
    saveSearchHistory([value, ...withoutDup].slice(0, 10));
  }

  function closeDesktopSearch() {
    desktopSearchOpen.value = false;
  }

  function toggleDesktopSearch() {
    desktopSearchOpen.value = !desktopSearchOpen.value;
  }

  function clearSearch() {
    // 使仍在途的搜索请求失效，避免清空后旧结果又回填
    searchSequence += 1;
    searchQuery.value = "";
    searchResults.value = [];
    searchError.value = null;
    searchActive.value = false;
  }

  async function runSearch() {
    return await doSearch(true);
  }

  async function doSearch(pushHistoryEnabled: boolean) {
    const query = searchQuery.value.trim();
    searchError.value = null;

    if (!query) {
      clearSearch();
      return;
    }

    const requestId = ++searchSequence;
    searchLoading.value = true;
    searchActive.value = true;

    if (pushHistoryEnabled) {
      pushSearchHistory(query);
    }

    try {
      const scopePath = searchScopeCurrent.value ? currentPath.value : "";
      const results = await filesService.searchFiles(query, searchMode.value, {
        type: searchType.value,
        path: scopePath,
      });
      if (requestId !== searchSequence) return;
      searchResults.value = results;
    } catch (err) {
      if (requestId !== searchSequence) return;
      searchError.value = err instanceof Error ? err.message : "搜索失败";
      searchResults.value = [];
    } finally {
      if (requestId === searchSequence) searchLoading.value = false;
    }
  }

  async function runDesktopSearch() {
    const query = searchQuery.value.trim();
    if (!query) {
      clearSearch();
      closeDesktopSearch();
      void nextTick().then(() => desktopSearchInputRef.value?.focus());
      return;
    }

    await runSearch();
    if (!searchError.value) closeDesktopSearch();
  }

  function clearDesktopSearch() {
    clearSearch();
    closeDesktopSearch();
    void nextTick().then(() => desktopSearchInputRef.value?.focus());
  }

  loadSearchHistory();

  return {
    searchQuery,
    searchResults,
    searchLoading,
    searchError,
    searchActive,
    searchContent,
    searchType,
    searchScopeCurrent,
    desktopSearchOpen,
    desktopSearchBoxRef,
    desktopSearchInputRef,
    searchHistory,
    searchMode,
    desktopSearchFiltersActive,
    loadSearchHistory,
    pushSearchHistory,
    closeDesktopSearch,
    toggleDesktopSearch,
    clearSearch,
    runSearch,
    doSearch,
    runDesktopSearch,
    clearDesktopSearch,
  };
}
