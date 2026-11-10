import { ref, type Ref } from "vue";
import type { FileInfo } from "../types";
import { filesService, MAX_TRANSFER_PATHS } from "../services/files.service";

export interface FileTransferDeps {
  isRestricted: Ref<boolean>;
  clearSelection: () => void;
  refresh: () => void | Promise<void>;
  notifySuccess: (message: string) => void;
  notifyError: (message: string) => void;
}

/**
 * 所有权转移对话框：条目筛选、上限校验、提交与失败恢复。
 *
 * dialogsRef 指向 FileBrowserDialogs 暴露的 finishTransfer；提交失败时由它
 * 恢复被转移对话框的按钮状态。
 */
export function useFileTransfer(deps: FileTransferDeps) {
  const showTransferDialog = ref(false);
  const transferItems = ref<FileInfo[]>([]);
  const dialogsRef = ref<{ finishTransfer: () => void } | null>(null);

  function openTransferDialog(items: FileInfo[]) {
    if (deps.isRestricted.value) return;
    const targets = items.filter((item) => item.path !== "");
    if (targets.length === 0) return;
    if (targets.length > MAX_TRANSFER_PATHS) {
      deps.notifyError(
        `一次最多转移 ${MAX_TRANSFER_PATHS} 个项目，请减少所选项目后重试`,
      );
      return;
    }
    transferItems.value = targets;
    showTransferDialog.value = true;
  }

  function closeTransferDialog() {
    showTransferDialog.value = false;
    transferItems.value = [];
  }

  async function submitTransfer(
    target: { id: string; username: string },
    message: string,
  ) {
    const paths = transferItems.value.map((item) => item.path);
    try {
      const result = await filesService.transferOwnership(
        paths,
        target.id,
        message || undefined,
      );
      deps.notifySuccess(
        `已把 ${result.transferred} 个条目转移给 ${result.target_username}`,
      );
      closeTransferDialog();
      deps.clearSelection();
      await deps.refresh();
    } catch (err) {
      deps.notifyError(err instanceof Error ? err.message : "转移失败");
      dialogsRef.value?.finishTransfer();
    }
  }

  function setDialogsRef(el: unknown) {
    dialogsRef.value = (el as { finishTransfer: () => void } | null) ?? null;
  }

  return {
    showTransferDialog,
    transferItems,
    setDialogsRef,
    openTransferDialog,
    closeTransferDialog,
    submitTransfer,
  };
}
