import { computed, type ComputedRef, type Ref } from "vue";
import type { FileInfo } from "../types";
import { fileIconKind } from "../utils/filePresentation";
import { sortBrowserItems, sortFiles, type SortState } from "../utils/fileSort";
import type { SearchResultFilter } from "./useFileSearch";

interface FileBrowserListDeps {
  files: Ref<FileInfo[]>;
  sortState: ComputedRef<SortState>;
  searchActive: Ref<boolean>;
  searchResults: Ref<FileInfo[]>;
  resultFilter: Ref<SearchResultFilter>;
  currentPath: Ref<string>;
  browseCommit: Ref<string | undefined>;
  libraryCards: ComputedRef<boolean>;
  loading: Ref<boolean>;
  error: Ref<string | null>;
  isRestricted: Ref<boolean>;
  scopeRoots: ComputedRef<string[]>;
  totalFiles: ComputedRef<number>;
}

/**
 * 文件浏览器的列表派生：目录排序、资料库卡片、搜索结果筛选与合并列表。
 *
 * 只负责「数据长什么样」，可见窗口与分页由 useProgressiveList 负责。
 */
export function useFileBrowserList(deps: FileBrowserListDeps) {
  const navigationListItems = computed<FileInfo[]>(() =>
    sortBrowserItems(deps.files.value, deps.sortState.value),
  );

  const showLibraryCards = computed(
    () =>
      deps.currentPath.value === "" &&
      !deps.browseCommit.value &&
      deps.libraryCards.value &&
      !deps.searchActive.value &&
      !deps.loading.value &&
      !deps.error.value &&
      (!deps.isRestricted.value || deps.scopeRoots.value.length === 0),
  );

  /** 根目录卡片只取一级子目录，并跟随列表当前的排序偏好。 */
  const libraryDirectories = computed(() =>
    navigationListItems.value.filter((file) => file.kind === "directory"),
  );

  /** 搜索结果按图标分类归入筛选桶（与搜索工具条的 chips 口径一致）。 */
  function searchResultBucket(file: FileInfo): SearchResultFilter {
    switch (fileIconKind(file)) {
      case "folder":
        return "folder";
      case "image":
        return "image";
      case "video":
        return "video";
      case "audio":
        return "audio";
      case "text":
      case "code":
      case "pdf":
        return "document";
      default:
        return "other";
    }
  }

  const filteredSearchResults = computed<FileInfo[]>(() => {
    if (deps.resultFilter.value === "all") return deps.searchResults.value;
    return deps.searchResults.value.filter(
      (file) => searchResultBucket(file) === deps.resultFilter.value,
    );
  });

  const sortedSearchResults = computed<FileInfo[]>(() =>
    sortFiles(filteredSearchResults.value, deps.sortState.value),
  );

  const activeList = computed(() =>
    deps.searchActive.value
      ? sortedSearchResults.value
      : navigationListItems.value,
  );

  // 当前目录的已知条目总数（含 "." 与 ".." 两个虚拟条目），用于进度提示。
  const knownTotal = computed(() =>
    deps.searchActive.value
      ? activeList.value.length
      : deps.totalFiles.value + 2,
  );

  return {
    navigationListItems,
    showLibraryCards,
    libraryDirectories,
    filteredSearchResults,
    sortedSearchResults,
    activeList,
    knownTotal,
  };
}
