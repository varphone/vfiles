import { computed, nextTick, watch, type Ref } from "vue";
import { useDocumentEventListener } from "./useDocumentEventListener";
import {
  firstScopeRoot,
  isPathWithinRoots,
  normalizeScopePath,
  scopedParentPath,
} from "../utils/sessionScope";

export interface DirectoryNavigationDeps {
  currentPath: Ref<string>;
  isRestricted: Ref<boolean>;
  scopeRoots: Ref<string[]>;
  searchActive: Ref<boolean>;
  clearSearch: () => void;
  expandedFilePath: Ref<string>;
  getSelectedCount: () => number;
  clearSelection: () => void;
  navigateStore: (path: string) => void;
  reload: (path: string) => void | Promise<void>;
  errorStatus: Ref<number | null>;
  requestedPath: Ref<string>;
  notifyError: (message: string) => void;
}

/**
 * 目录导航与滚动记忆。
 *
 * - 目录切换的唯一入口：范围校验、清选择、stash/restore 滚动位置；
 * - 403 与受限范围变化时回到范围根，失败不循环；
 * - 列表真实滚动容器是 .desktop-list-shell，滚动事件持续追踪，避免点击竞态读到 0。
 */
export function useDirectoryNavigation(deps: DirectoryNavigationDeps) {
  const scrollMemory = new Map<string, number>();
  const MAX_SCROLL_MEMORY = 50;
  let pendingScrollRestore: number | null = null;
  let lastScrollTop = 0;

  useDocumentEventListener(
    "scroll",
    (event) => {
      const target = event.target as HTMLElement | null;
      if (target?.classList?.contains("desktop-list-shell")) {
        lastScrollTop = target.scrollTop;
      }
    },
    true,
  );

  function stashScroll(path: string) {
    scrollMemory.set(path, lastScrollTop);
    if (scrollMemory.size > MAX_SCROLL_MEMORY) {
      const oldest = scrollMemory.keys().next().value;
      if (oldest !== undefined) scrollMemory.delete(oldest);
    }
  }

  function flushScrollRestore() {
    if (pendingScrollRestore == null) return;
    const restore = pendingScrollRestore;
    pendingScrollRestore = null;
    const apply = () => {
      const shell = document.querySelector<HTMLElement>(".desktop-list-shell");
      if (shell) {
        shell.scrollTop = restore;
        lastScrollTop = restore;
      }
    };
    let attempts = 0;
    const timer = setInterval(() => {
      attempts += 1;
      if (attempts > 6) {
        clearInterval(timer);
        return;
      }
      const shell = document.querySelector<HTMLElement>(".desktop-list-shell");
      if (shell && Math.abs(shell.scrollTop - restore) > 5) {
        shell.scrollTop = restore;
      }
    }, 100);
    requestAnimationFrame(() => {
      requestAnimationFrame(apply);
    });
  }

  function isNavigationAllowed(path: string): boolean {
    return (
      !deps.isRestricted.value ||
      isPathWithinRoots(path, deps.scopeRoots.value)
    );
  }

  function navigateTo(path: string) {
    const normalized = normalizeScopePath(path);
    if (!isNavigationAllowed(normalized)) {
      const root = firstScopeRoot(deps.scopeRoots.value);
      deps.notifyError("无权访问该目录");
      if (!root || normalizeScopePath(deps.currentPath.value) === root) return;
      navigateTo(root);
      return;
    }
    stashScroll(deps.currentPath.value);
    pendingScrollRestore = scrollMemory.get(normalized) ?? null;
    deps.expandedFilePath.value = "";
    // 选择属于当前目录：切目录后旧选择既不可见也不可操作。
    if (deps.getSelectedCount() > 0) deps.clearSelection();
    deps.navigateStore(normalized);
    void nextTick().then(flushScrollRestore);
  }

  /** 面包屑跳转：处于搜索结果时先退出搜索，再进入目标目录。 */
  function handleBreadcrumbNavigate(path: string) {
    if (deps.searchActive.value) deps.clearSearch();
    navigateTo(path);
  }

  function refresh() {
    deps.expandedFilePath.value = "";
    return deps.reload(deps.currentPath.value);
  }

  const parentPath = computed<string | null>(() => {
    const cur = normalizeScopePath(deps.currentPath.value);
    if (!cur) return null;
    if (deps.isRestricted.value) {
      return scopedParentPath(cur, deps.scopeRoots.value);
    }
    const parts = cur.split("/").filter(Boolean);
    parts.pop();
    return parts.join("/");
  });

  function goBack() {
    if (parentPath.value == null) return;
    deps.expandedFilePath.value = "";
    navigateTo(parentPath.value);
  }

  function goRoot() {
    navigateTo(
      deps.isRestricted.value
        ? firstScopeRoot(deps.scopeRoots.value)
        : "",
    );
  }

  // 403 = 当前目录在会话范围之外：提示后回到第一个范围根，失败不循环。
  let handlingForbidden = false;
  watch(
    () => deps.errorStatus.value,
    (status) => {
      if (status !== 403) return;
      deps.notifyError("无权访问该目录");
      if (!deps.isRestricted.value || handlingForbidden) return;
      const root = firstScopeRoot(deps.scopeRoots.value);
      if (
        !root ||
        normalizeScopePath(deps.requestedPath.value) === root
      ) {
        return;
      }
      handlingForbidden = true;
      navigateTo(root);
      void nextTick().then(() => {
        handlingForbidden = false;
      });
    },
  );

  // 受限范围变化（如重新登录）时把范围外路径拉回范围根。
  watch(
    [deps.isRestricted, deps.scopeRoots],
    () => {
      if (!deps.isRestricted.value) return;
      const roots = deps.scopeRoots.value;
      if (roots.length === 0) return;
      if (isPathWithinRoots(deps.currentPath.value, roots)) return;
      navigateTo(firstScopeRoot(roots));
    },
    { deep: true },
  );

  return {
    navigateTo,
    handleBreadcrumbNavigate,
    refresh,
    goBack,
    goRoot,
    parentPath,
  };
}
