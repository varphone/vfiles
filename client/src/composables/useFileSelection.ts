import { computed, ref, type Ref } from "vue";
import { useAppStore } from "../stores/app.store";
import { filesService } from "../services/files.service";
import { confirmDialog } from "./dialog";
import { parentDirectoryPath } from "../utils/filePaths";
import type { FileInfo } from "../types";

export interface FileSelectionDeps {
  searchActive: Ref<boolean>;
  /** 当前视图可见条目（含 `.`/`..` 快捷项，函数内部会过滤）。 */
  getVisibleItems: () => FileInfo[];
  /** 把选中路径解析回 FileInfo 的来源池（当前目录文件或搜索结果）。 */
  getSelectionPool: () => FileInfo[];
  setActivePath: (path: string) => void;
  currentPath: Ref<string>;
  browseCommit: Ref<string | undefined>;
  refresh: () => void | Promise<void>;
  doSearch: (pushHistoryEnabled: boolean) => Promise<void> | void;
  openMoveDialog: (items: FileInfo[], initialPath: string) => void;
  renameEntry: (file: FileInfo) => Promise<void> | void;
}

function isShortcut(_file: FileInfo): boolean {
  // 列表已不再注入“.”“..”快捷项；保留该判断以便调用点语义稳定
  return false;
}

/**
 * 批量选择与批量操作：选择状态、Shift 范围/Ctrl 加选、全选，以及批量
 * 下载/删除/移动/重命名。
 *
 * 从 FileBrowser 抽出，视图相关数据通过 `deps` 注入，便于测试。
 */
export function useFileSelection(deps: FileSelectionDeps) {
  const appStore = useAppStore();

  const batchMode = ref(false);
  const selectedPaths = ref<Set<string>>(new Set());
  /** 最近一次点击的条目路径，用于 Shift 范围选择。 */
  const lastSelectedPath = ref<string>("");

  const selectedCount = computed(() => selectedPaths.value.size);

  /** 当前可见的、可选择的真实条目（排除 `.`/`..` 快捷项）。 */
  function selectableItems(): FileInfo[] {
    return deps.getVisibleItems().filter((file) => !isShortcut(file));
  }

  function toggleBatchMode() {
    batchMode.value = !batchMode.value;
    if (!batchMode.value) {
      clearSelection();
    }
  }

  function toggleSelect(file: FileInfo) {
    deps.setActivePath(file.path);
    lastSelectedPath.value = file.path;
    const next = new Set(selectedPaths.value);
    if (next.has(file.path)) {
      next.delete(file.path);
    } else {
      next.add(file.path);
    }
    selectedPaths.value = next;
  }

  /**
   * Shift/Ctrl(⌘) 点击：Shift 选中最近一次点击到当前项的连续区间，
   * Ctrl(⌘) 切换单项选择；两者都会自动进入批量模式。
   */
  function handleModifierSelect(payload: {
    file: FileInfo;
    shift: boolean;
    meta: boolean;
  }) {
    const file = payload.file;
    if (isShortcut(file)) return;

    if (payload.shift && lastSelectedPath.value) {
      const list = selectableItems();
      const from = list.findIndex(
        (item) => item.path === lastSelectedPath.value,
      );
      const to = list.findIndex((item) => item.path === file.path);
      if (from !== -1 && to !== -1) {
        const [start, end] = from <= to ? [from, to] : [to, from];
        const next = new Set(selectedPaths.value);
        for (let index = start; index <= end; index += 1) {
          next.add(list[index].path);
        }
        selectedPaths.value = next;
        batchMode.value = true;
        deps.setActivePath(file.path);
        return;
      }
    }

    batchMode.value = true;
    toggleSelect(file);
  }

  function clearSelection() {
    selectedPaths.value = new Set();
  }

  function replaceSelectedPath(oldPath: string, newPath: string) {
    if (!selectedPaths.value.has(oldPath)) return;
    const next = new Set(selectedPaths.value);
    next.delete(oldPath);
    next.add(newPath);
    selectedPaths.value = next;
  }

  /** 右键未选中项时把选择收敛为该项（仅在批量模式下）。 */
  function narrowSelectionTo(path: string) {
    if (batchMode.value && !selectedPaths.value.has(path)) {
      selectedPaths.value = new Set([path]);
      lastSelectedPath.value = path;
    }
  }

  function selectAllVisible() {
    const next = new Set(selectedPaths.value);
    for (const file of selectableItems()) {
      next.add(file.path);
    }
    selectedPaths.value = next;
  }

  function toggleSelectAll() {
    if (!batchMode.value) return;
    const selectable = selectableItems();
    const allSelected =
      selectable.length > 0 &&
      selectable.every((file) => selectedPaths.value.has(file.path));

    if (allSelected) {
      clearSelection();
    } else {
      selectAllVisible();
    }
  }

  function getSelectedItems(): FileInfo[] {
    const pool = deps.getSelectionPool();
    const map = new Map(pool.map((file) => [file.path, file] as const));
    const items: FileInfo[] = [];
    for (const path of selectedPaths.value) {
      const item = map.get(path);
      if (item) items.push(item);
    }
    return items;
  }

  async function batchDownload() {
    const items = getSelectedItems();
    if (items.length === 0) return;

    const files = items.filter((file) => file.kind !== "directory");
    const folders = items.filter((file) => file.kind === "directory");
    const totalCount = files.length + folders.length;

    if (totalCount > 10) {
      const ok = await confirmDialog({
        title: "批量下载",
        message: `将开始下载 ${totalCount} 个项目，可能会被浏览器拦截弹窗。继续吗？`,
        confirmText: "继续下载",
      });
      if (!ok) return;
    }

    for (const file of files) {
      filesService.downloadFile(file.path, deps.browseCommit.value);
    }
    for (const folder of folders) {
      filesService.downloadFolder(folder.path, deps.browseCommit.value);
    }

    appStore.success(`已开始下载 ${totalCount} 个项目`);
  }

  async function batchDelete() {
    const items = getSelectedItems();
    if (items.length === 0) return;

    const ok = await confirmDialog({
      title: "批量删除",
      message: `确定要删除 ${items.length} 项吗？此操作会生成一次或多次提交。`,
      confirmText: "删除",
      danger: true,
    });
    if (!ok) return;

    try {
      for (const file of items) {
        await filesService.deleteFile(file.path, "批量删除");
      }
      appStore.success("批量删除完成");
      clearSelection();
      await deps.refresh();
      if (deps.searchActive.value) {
        await deps.doSearch(false);
      }
    } catch (err) {
      appStore.error(err instanceof Error ? err.message : "批量删除失败");
    }
  }

  function batchMove() {
    const items = getSelectedItems();
    if (items.length === 0) return;

    deps.openMoveDialog(
      items,
      deps.currentPath.value || parentDirectoryPath(items[0]?.path || ""),
    );
  }

  async function renameSelected() {
    const items = getSelectedItems();
    if (items.length !== 1) return;
    await deps.renameEntry(items[0]);
  }

  return {
    batchMode,
    selectedPaths,
    lastSelectedPath,
    selectedCount,
    toggleBatchMode,
    toggleSelect,
    handleModifierSelect,
    clearSelection,
    replaceSelectedPath,
    narrowSelectionTo,
    selectAllVisible,
    toggleSelectAll,
    getSelectedItems,
    batchDownload,
    batchDelete,
    batchMove,
    renameSelected,
  };
}
