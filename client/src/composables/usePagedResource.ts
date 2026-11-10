import { computed, ref, shallowRef } from "vue";

/**
 * 页码式分页资源（表格 + 上/下一页）。
 *
 * 与 `useListResource` 的区别：这里是「第 N 页」而不是「追加更多」，并保留了
 * 页码越界时回退到最后一页的语义（删掉最后一页的所有条目后仍能停在有效页）。
 * 同样用递增序号丢弃过期响应，避免连点页码时旧结果覆盖新页。
 */
export interface PagedResourceOptions<T> {
  fetchPage: (params: { limit: number; offset: number }) => Promise<{
    items: T[];
    total: number;
  }>;
  pageSize: number;
  onError?: (message: string) => void;
}

export function usePagedResource<T>(options: PagedResourceOptions<T>) {
  const items = shallowRef<T[]>([]);
  const total = ref(0);
  const page = ref(0);
  const loading = ref(false);
  const error = ref("");

  const pageCount = computed(() =>
    Math.max(1, Math.ceil(total.value / options.pageSize)),
  );

  let sequence = 0;

  async function load(requestedPage = page.value): Promise<void> {
    const seq = ++sequence;
    loading.value = true;
    error.value = "";
    try {
      const fetchAt = (target: number) =>
        options.fetchPage({
          limit: options.pageSize,
          offset: target * options.pageSize,
        });

      let target = Math.max(0, requestedPage);
      let result = await fetchAt(target);
      const lastPage = Math.max(0, Math.ceil(result.total / options.pageSize) - 1);
      // 越界（例如删完最后一页）时回退到最后一页再取一次。
      if (target > lastPage) {
        target = lastPage;
        result = await fetchAt(target);
      }
      if (seq !== sequence) return;

      page.value = target;
      total.value = result.total;
      items.value = result.items;
    } catch (e) {
      if (seq !== sequence) return;
      error.value = e instanceof Error ? e.message : "加载失败";
      options.onError?.(error.value);
    } finally {
      if (seq === sequence) loading.value = false;
    }
  }

  function goToPage(next: number): void {
    if (next < 0 || next >= pageCount.value) return;
    void load(next);
  }

  return {
    items,
    total,
    page,
    pageCount,
    loading,
    error,
    load,
    /** 重新加载当前页。 */
    reload: () => load(page.value),
    goToPage,
  };
}
