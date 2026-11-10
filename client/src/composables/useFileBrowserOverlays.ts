import { type Ref } from "vue";

interface OverlayDeps {
  contextMenu: Ref<{ show: boolean }>;
  previewOpen: Ref<boolean>;
  showShortcuts: Ref<boolean>;
  showUploader: Ref<boolean>;
  showHistory: Ref<boolean>;
  showShareDialog: Ref<boolean>;
  showMoveDialog: Ref<boolean>;
  showDetailsDialog: Ref<boolean>;
  showTransferDialog: Ref<boolean>;
  dirManagerOpen: Ref<boolean>;
  closeShareDialog: () => void;
  loadSharedPaths: (force?: boolean) => void | Promise<void>;
}

/**
 * 覆盖层守卫：任何弹窗/菜单打开时，列表快捷键与整窗拖放应让位。
 *
 * 同时处理分享对话框关闭后的徽标强制刷新。
 */
export function useFileBrowserOverlays(deps: OverlayDeps) {
  function anyOverlayOpen(): boolean {
    return (
      deps.contextMenu.value.show ||
      deps.previewOpen.value ||
      deps.showShortcuts.value ||
      deps.showUploader.value ||
      deps.showHistory.value ||
      deps.showShareDialog.value ||
      deps.showMoveDialog.value ||
      deps.showDetailsDialog.value ||
      deps.showTransferDialog.value ||
      deps.dirManagerOpen.value ||
      // 全局确认框用 DOM 判定，避免上一个对话框未结算时的模块级残留。
      document.querySelector(".modal.is-active") !== null
    );
  }

  /** 分享对话框关闭后强制刷新徽标集合（期间可能新建或停止分享）。 */
  function closeShareAndRefresh() {
    deps.closeShareDialog();
    void deps.loadSharedPaths(true);
  }

  return { anyOverlayOpen, closeShareAndRefresh };
}
