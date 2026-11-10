import { type Ref } from "vue";
import type { FileInfo } from "../types";

interface ItemActionDeps {
  isMobile: Ref<boolean>;
  batchMode: Ref<boolean>;
  desktopActivePath: Ref<string>;
  lastSelectedPath: Ref<string>;
  expandedFilePath: Ref<string>;
  searchActive: Ref<boolean>;
  searchResults: Ref<FileInfo[]>;
  showDetailsDialog: Ref<boolean>;
  navigateTo: (path: string) => void;
  clearSearch: () => void;
  openPreview: (path: string) => void;
  enqueueDownload: (kind: "file" | "folder", path: string) => void;
  findActiveItem: () => FileInfo | undefined;
}

/**
 * 条目点击/双击与预览、打开目录、下载、快速预览动作。
 */
export function useFileBrowserItemActions(deps: ItemActionDeps) {
  function handleItemClick(file: FileInfo) {
    if (file.kind === "directory") {
      handleOpenFolder(file);
      return;
    }

    if (!deps.isMobile.value && !deps.batchMode.value) {
      deps.desktopActivePath.value = file.path;
      // 记录锚点，便于随后 Shift 点击做范围选择
      deps.lastSelectedPath.value = file.path;
      // 点击行后把焦点交给列表容器：行本身不可聚焦，若焦点仍停在工具栏按钮上，
      // Shift+F10 / 方向键等列表快捷键会被「交互控件」守卫挡掉。
      document
        .querySelector<HTMLElement>(".desktop-list-shell")
        ?.focus({ preventScroll: true });
      return;
    }

    if (deps.expandedFilePath.value === file.path) {
      deps.expandedFilePath.value = "";
    } else {
      deps.expandedFilePath.value = file.path;
    }
  }

  function handlePreview(file: FileInfo) {
    deps.openPreview(file.path);
  }

  /** 详细信息弹窗里的「预览」：关掉详情再打开预览。 */
  function onDetailsPreview(file: FileInfo) {
    deps.showDetailsDialog.value = false;
    handlePreview(file);
  }

  function handleOpenFolder(file: FileInfo) {
    if (file.kind !== "directory") return;
    if (deps.searchActive.value) deps.clearSearch();
    deps.navigateTo(file.path);
  }

  function handleDownload(file: FileInfo) {
    // 通过下载队列提供进度、错误反馈与取消能力。
    deps.enqueueDownload(
      file.kind === "directory" ? "folder" : "file",
      file.path,
    );
  }

  /** 空格快速预览：预览活动行。 */
  function quickLook() {
    const active = deps.findActiveItem();
    if (active) deps.openPreview(active.path);
  }

  /** 搜索框 ↓：进入结果列表首项。 */
  function enterSearchResults() {
    const first = deps.searchResults.value[0];
    if (!first) return;
    deps.desktopActivePath.value = first.path;
  }

  return {
    handleItemClick,
    handlePreview,
    onDetailsPreview,
    handleOpenFolder,
    handleDownload,
    quickLook,
    enterSearchResults,
  };
}
