import {
  computed,
  nextTick,
  onBeforeUnmount,
  ref,
  watch,
  type ComputedRef,
  type Ref,
} from "vue";
import type { FileInfo } from "../types";

interface ProgressiveListDeps {
  activeList: ComputedRef<FileInfo[]>;
  isMobile: Ref<boolean>;
  searchActive: Ref<boolean>;
  searchHasMore: Ref<boolean>;
  loadMoreSearchResults: () => void | Promise<void>;
  serverHasMore: ComputedRef<boolean>;
  loadMoreServer: () => void | Promise<void>;
  /** 目录、搜索词、结果数等变化时把可见窗口复位。 */
  resetKey: ComputedRef<string>;
}

const INITIAL_VISIBLE_COUNT = 40;
const VISIBLE_CHUNK_COUNT = 30;

/**
 * 本地分批渲染 + 服务端分页的哨兵加载。
 *
 * 先补齐本地已加载条目的渲染分片；本地补齐后，如果目录/搜索还有服务端下一页，
 * 再按需拉取。哨兵仍在视口内时会继续递归，避免 IntersectionObserver 在同一
 * 交叉状态下不再回调。
 */
export function useProgressiveList(deps: ProgressiveListDeps) {
  const visibleCount = ref(INITIAL_VISIBLE_COUNT);
  const loadMoreSentinel = ref<HTMLElement | null>(null);
  let observer: IntersectionObserver | null = null;

  const hasMoreLocal = computed(
    () => visibleCount.value < deps.activeList.value.length,
  );
  const hasMore = computed(() => {
    if (deps.searchActive.value) {
      return hasMoreLocal.value || deps.searchHasMore.value;
    }
    return hasMoreLocal.value || deps.serverHasMore.value;
  });

  function bumpVisibleCount() {
    visibleCount.value = Math.min(
      deps.activeList.value.length,
      visibleCount.value + VISIBLE_CHUNK_COUNT,
    );
  }

  function maybeLoadMore() {
    if (!hasMore.value) return;
    const sentinel = loadMoreSentinel.value;
    if (!sentinel) return;

    const rect = sentinel.getBoundingClientRect();
    const viewportHeight =
      window.innerHeight || document.documentElement.clientHeight || 0;
    if (rect.top > viewportHeight + 160) return;

    if (hasMoreLocal.value) {
      const before = visibleCount.value;
      bumpVisibleCount();
      if (visibleCount.value === before) return;
      void nextTick().then(() => maybeLoadMore());
      return;
    }

    if (deps.searchActive.value) {
      if (deps.searchHasMore.value) void deps.loadMoreSearchResults();
      return;
    }

    if (deps.serverHasMore.value) void deps.loadMoreServer();
  }

  function setupObserver() {
    observer?.disconnect();
    observer = null;
    if (!("IntersectionObserver" in window)) return;
    if (!loadMoreSentinel.value) return;
    observer = new IntersectionObserver(
      (entries) => {
        if (!entries.some((entry) => entry.isIntersecting)) return;
        maybeLoadMore();
      },
      { root: null, threshold: 0.1, rootMargin: "120px" },
    );
    observer.observe(loadMoreSentinel.value);
  }

  watch(deps.resetKey, () => {
    visibleCount.value = INITIAL_VISIBLE_COUNT;
    void nextTick().then(() => setupObserver());
  });

  watch([deps.isMobile, loadMoreSentinel], () => {
    void nextTick().then(() => setupObserver());
  });

  // 服务端分页追加数据后，若哨兵仍在视口内则继续拉取下一页。
  watch(
    () => deps.activeList.value.length,
    () => {
      void nextTick().then(() => maybeLoadMore());
    },
  );

  onBeforeUnmount(() => {
    observer?.disconnect();
    observer = null;
  });

  function setLoadMoreSentinel(element: unknown) {
    loadMoreSentinel.value = (element as HTMLElement | null) ?? null;
  }

  return {
    visibleCount,
    hasMore,
    hasMoreLocal,
    setLoadMoreSentinel,
  };
}
