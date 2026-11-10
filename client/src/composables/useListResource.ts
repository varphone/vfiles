import { ref, shallowRef } from "vue";

/**
 * 列表分页资源：统一「旧响应覆盖新结果」的防护。
 *
 * 各页面原先各写一份 reload()，多数没有序号或取消：快速输入搜索词、连点页码、
 * 切换筛选时，先发出的请求后返回会把新结果覆盖掉。
 *
 * 这里用递增序号判定（不需要 AbortController 也能保证「只认最后一次」），
 * 并提供首次加载 / 追加加载 / 错误 / 是否还有更多四个状态。
 */
export interface ListResourceOptions<T> {
  /** 拉取一页；offset 由调用方按已加载条数给出。 */
  fetchPage: (params: {
    limit: number;
    offset: number;
    signal: AbortSignal;
  }) => Promise<{ items: T[]; total: number }>;
  pageSize?: number;
  /** 追加时用于去重的稳定键（例如条目 id）；不给则不去重。 */
  key?: (item: T) => string | number;
}

export function useListResource<T>(options: ListResourceOptions<T>) {
  const pageSize = options.pageSize ?? 50;
  const items = shallowRef<T[]>([]);
  const total = ref(0);
  const loading = ref(false);
  const loadingMore = ref(false);
  const error = ref<string | null>(null);

  /** 每次发起请求自增；响应回来时若已不是最新一次，直接丢弃。 */
  let sequence = 0;
  let controller: AbortController | null = null;

  function begin() {
    sequence += 1;
    controller?.abort();
    controller = new AbortController();
    return { seq: sequence, signal: controller.signal };
  }

  async function reload(): Promise<void> {
    const { seq, signal } = begin();
    loading.value = true;
    loadingMore.value = false;
    error.value = null;

    try {
      const page = await options.fetchPage({ limit: pageSize, offset: 0, signal });
      if (seq !== sequence) return;
      items.value = page.items;
      total.value = page.total;
    } catch (e) {
      if (seq !== sequence) return;
      if (e instanceof DOMException && e.name === "AbortError") return;
      error.value = e instanceof Error ? e.message : "加载失败";
    } finally {
      if (seq === sequence) loading.value = false;
    }
  }

  async function loadMore(): Promise<void> {
    if (loading.value || loadingMore.value) return;
    if (items.value.length >= total.value) return;

    const { seq, signal } = begin();
    loadingMore.value = true;

    try {
      const page = await options.fetchPage({
        limit: pageSize,
        offset: items.value.length,
        signal,
      });
      if (seq !== sequence) return;
      const keyOf = options.key;
      const appended = keyOf
        ? (() => {
            const seen = new Set(items.value.map((item) => keyOf(item)));
            return page.items.filter((item) => {
              const key = keyOf(item);
              if (seen.has(key)) return false;
              seen.add(key);
              return true;
            });
          })()
        : page.items;
      items.value = [...items.value, ...appended];
      total.value = page.total;
    } catch (e) {
      if (seq !== sequence) return;
      if (e instanceof DOMException && e.name === "AbortError") return;
      error.value = e instanceof Error ? e.message : "加载更多失败";
    } finally {
      if (seq === sequence) loadingMore.value = false;
    }
  }

  /** 清空（切换筛选时先清再拉，避免旧数据闪一下）。 */
  function reset(): void {
    sequence += 1;
    controller?.abort();
    items.value = [];
    total.value = 0;
    error.value = null;
  }

  return {
    items,
    total,
    loading,
    loadingMore,
    error,
    reload,
    loadMore,
    reset,
    hasMore: () => items.value.length < total.value,
  };
}
