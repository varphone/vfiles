import { ref, watch, type Ref } from "vue";

const SHARED_PATHS_TTL_MS = 30_000;

/**
 * 已分享路径集合（列表「共享」列徽标）。
 *
 * 分享接口返回整个可见范围的路径集合；账号级数据在 30s 内复用，切换目录
 * 不重复请求。请求失败按「没有分享」处理，不让徽标拖垮文件列表。
 */
export function useSharedPaths(
  currentPath: Ref<string>,
  fetchPaths: () => Promise<string[]>,
) {
  const sharedPaths = ref<Set<string>>(new Set());
  let request = 0;
  let loadedAt = 0;

  async function loadSharedPaths(force = false) {
    if (!force && loadedAt > 0 && Date.now() - loadedAt < SHARED_PATHS_TTL_MS) {
      return;
    }
    const id = ++request;
    try {
      const paths = await fetchPaths();
      if (id !== request) return;
      sharedPaths.value = new Set(
        (Array.isArray(paths) ? paths : []).map((path) =>
          String(path).replace(/^\/+/, ""),
        ),
      );
      loadedAt = Date.now();
    } catch {
      if (id !== request) return;
      sharedPaths.value = new Set();
    }
  }

  watch(
    currentPath,
    () => {
      void loadSharedPaths();
    },
    { immediate: true },
  );

  return { sharedPaths, loadSharedPaths };
}
