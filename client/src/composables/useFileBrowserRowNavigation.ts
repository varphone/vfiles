import { nextTick, ref, type ComputedRef, type Ref } from "vue";
import type { FileInfo } from "../types";
import { columnsFromElements } from "../utils/gridLayout";

interface RowNavigationDeps {
  searchActive: Ref<boolean>;
  sortedSearchResults: ComputedRef<FileInfo[]>;
  navigationListItems: ComputedRef<FileInfo[]>;
  activeList: ComputedRef<FileInfo[]>;
  visibleCount: Ref<number>;
  viewMode: ComputedRef<"list" | "grid">;
  desktopActivePath: Ref<string>;
  lastSelectedPath: Ref<string>;
  selectedPaths: Ref<Set<string>>;
  batchMode: Ref<boolean>;
  canWrite: ComputedRef<boolean>;
  toggleSelect: (file: FileInfo) => void;
  handleModifierSelect: (payload: {
    file: FileInfo;
    shift: boolean;
    meta: boolean;
  }) => void;
}

/**
 * 键盘行导航、活动项解析与按键定位（type-ahead）。
 *
 * 方向键/Home/End/PageUp/PageDown、Shift 区间选择、网格列步长、滚动补齐与
 * 800ms 按键前缀都集中在这里；列表数据由调用方注入。
 */
export function useFileBrowserRowNavigation(deps: RowNavigationDeps) {
  function findActiveItem(): FileInfo | undefined {
    const list = deps.searchActive.value
      ? deps.sortedSearchResults.value
      : deps.navigationListItems.value;

    if (deps.desktopActivePath.value) {
      const active = list.find(
        (file) => file.path === deps.desktopActivePath.value,
      );
      if (active) return active;
    }

    if (deps.selectedPaths.value.size === 1) {
      const [only] = deps.selectedPaths.value;
      const selected = list.find((file) => file.path === only);
      if (selected) return selected;
    }

    return undefined;
  }

  /** 网格视图下上下键的步长：按实际渲染的卡片推断列数。 */
  function gridColumnStep(): number {
    if (typeof document === "undefined") return 1;
    const cards = Array.from(
      document.querySelectorAll(".file-grid .file-card"),
    );
    return cards.length > 0 ? columnsFromElements(cards) : 1;
  }

  function pageStep(): number {
    if (typeof document === "undefined") return 10;
    const shell = document.querySelector<HTMLElement>(".desktop-list-shell");
    if (!shell) return 10;

    if (deps.viewMode.value === "grid") {
      const cards = Array.from(
        document.querySelectorAll<HTMLElement>(".file-grid .file-card"),
      );
      if (cards.length === 0) return 10;
      const columns = columnsFromElements(cards);
      const cardTop = cards[0].getBoundingClientRect().top;
      const nextRow = cards[columns];
      const rowHeight = nextRow
        ? nextRow.getBoundingClientRect().top - cardTop
        : cards[0].getBoundingClientRect().height + 12;
      if (rowHeight <= 0) return 10;
      const visibleRows = Math.max(
        1,
        Math.floor(shell.clientHeight / rowHeight),
      );
      return Math.max(1, visibleRows * columns);
    }

    const rows = Array.from(
      document.querySelectorAll<HTMLElement>(
        "tr.desktop-file-row, .file-item",
      ),
    );
    if (rows.length < 2) return 10;
    const rowHeight =
      rows[1].getBoundingClientRect().top - rows[0].getBoundingClientRect().top;
    if (rowHeight <= 0) return 10;
    return Math.max(1, Math.floor(shell.clientHeight / rowHeight));
  }

  function scrollActiveIntoView(path: string) {
    if (typeof document === "undefined") return;
    const selector = `[data-vfiles-path="${CSS.escape(path)}"]`;
    const element = document.querySelector<HTMLElement>(selector);
    element?.scrollIntoView?.({ block: "nearest" });
  }

  function moveActiveRow(key: string, shift: boolean) {
    const list = deps.searchActive.value
      ? deps.sortedSearchResults.value
      : deps.navigationListItems.value;
    if (list.length === 0) return;

    const currentIndex = list.findIndex(
      (item) => item.path === deps.desktopActivePath.value,
    );
    const rowStep = deps.viewMode.value === "grid" ? gridColumnStep() : 1;
    let nextIndex: number;
    switch (key) {
      case "ArrowUp":
        nextIndex = currentIndex <= 0 ? 0 : Math.max(0, currentIndex - rowStep);
        break;
      case "ArrowDown":
        nextIndex =
          currentIndex === -1
            ? 0
            : Math.min(list.length - 1, currentIndex + rowStep);
        break;
      case "ArrowLeft":
        nextIndex = currentIndex <= 0 ? 0 : currentIndex - 1;
        break;
      case "ArrowRight":
        nextIndex =
          currentIndex === -1
            ? 0
            : Math.min(list.length - 1, currentIndex + 1);
        break;
      case "Home":
        nextIndex = 0;
        break;
      case "End":
        nextIndex = list.length - 1;
        break;
      case "PageUp":
        nextIndex = Math.max(
          0,
          (currentIndex === -1 ? 0 : currentIndex) - pageStep(),
        );
        break;
      case "PageDown":
        nextIndex =
          currentIndex === -1
            ? 0
            : Math.min(list.length - 1, currentIndex + pageStep());
        break;
      default:
        return;
    }

    const next = list[nextIndex];
    if (!next) return;

    const previousPath = deps.desktopActivePath.value;
    const previousIsReal = list.some((item) => item.path === previousPath);

    deps.desktopActivePath.value = next.path;
    if (shift) {
      if (!deps.lastSelectedPath.value && previousIsReal) {
        deps.lastSelectedPath.value = previousPath;
      }
      deps.handleModifierSelect({ file: next, shift: true, meta: false });
    } else if (deps.batchMode.value) {
      deps.selectedPaths.value = new Set([next.path]);
    }

    if (nextIndex >= deps.visibleCount.value) {
      deps.visibleCount.value = Math.min(
        deps.activeList.value.length,
        nextIndex + 1,
      );
    }
    void nextTick().then(() => scrollActiveIntoView(next.path));
  }

  const typeAheadPrefix = ref("");
  let typeAheadTimer: number | undefined;

  function clearTypeAhead() {
    typeAheadPrefix.value = "";
    if (typeAheadTimer) {
      window.clearTimeout(typeAheadTimer);
      typeAheadTimer = undefined;
    }
  }

  function handleTypeAhead(character: string): boolean {
    const list = deps.searchActive.value
      ? deps.sortedSearchResults.value
      : deps.navigationListItems.value;
    if (list.length === 0) return false;

    typeAheadPrefix.value += character;
    if (typeAheadTimer) window.clearTimeout(typeAheadTimer);
    typeAheadTimer = window.setTimeout(() => {
      typeAheadPrefix.value = "";
    }, 800);

    const prefix = typeAheadPrefix.value.toLowerCase();
    const startIndex = Math.max(
      0,
      list.findIndex((item) => item.path === deps.desktopActivePath.value),
    );
    const ordered = [...list.slice(startIndex), ...list.slice(0, startIndex)];
    const match = ordered.find((item) =>
      item.name.toLowerCase().startsWith(prefix),
    );
    if (!match) {
      typeAheadPrefix.value = "";
      if (typeAheadTimer) window.clearTimeout(typeAheadTimer);
      return false;
    }

    const index = list.findIndex((item) => item.path === match.path);
    deps.desktopActivePath.value = match.path;
    if (deps.batchMode.value) {
      deps.selectedPaths.value = new Set([match.path]);
    }
    if (index >= deps.visibleCount.value) {
      deps.visibleCount.value = Math.min(
        deps.activeList.value.length,
        index + 1,
      );
    }
    void nextTick().then(() => scrollActiveIntoView(match.path));
    return true;
  }

  function toggleActiveRowSelection() {
    if (!deps.canWrite.value) return;
    const active = findActiveItem();
    if (!active) return;
    if (!deps.batchMode.value) deps.batchMode.value = true;
    deps.toggleSelect(active);
  }

  function handleRowSelect(file: FileInfo) {
    if (!deps.canWrite.value) return;
    deps.toggleSelect(file);
  }

  return {
    findActiveItem,
    moveActiveRow,
    typeAheadPrefix,
    clearTypeAhead,
    handleTypeAhead,
    toggleActiveRowSelection,
    handleRowSelect,
  };
}
