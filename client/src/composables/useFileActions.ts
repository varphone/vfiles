import { ref, type Ref } from "vue";
import type { FileInfo } from "../types";
import { confirmDialog } from "./dialog";

export interface FileActionsDeps {
  canWrite: Ref<boolean>;
  canShare: Ref<boolean>;
  currentPath: Ref<string>;
  searchActive: Ref<boolean>;
  desktopActivePath: Ref<string>;
  deletePath: (path: string, detail: string) => Promise<void>;
  promptCreateDirectory: (path: string) => void | Promise<void>;
  openMoveForEntry: (file: FileInfo) => void;
  doSearch: (pushHistoryEnabled: boolean) => void | Promise<void>;
  notifySuccess: (message: string) => void;
  notifyError: (message: string) => void;
}

/**
 * 条目级动作与对应弹窗状态：删除、新建文件夹、移动、分享、历史、详情。
 *
 * 从 FileBrowser 抽出后，主组件只负责把服务/store 能力注入进来；
 * 弹窗可见性仍由这里持有并返回给模板与 anyOverlayOpen 判断。
 */
export function useFileActions(deps: FileActionsDeps) {
  const selectedFile = ref<FileInfo | null>(null);
  const detailsFile = ref<FileInfo | null>(null);
  const showHistory = ref(false);
  const showShareDialog = ref(false);
  const showDetailsDialog = ref(false);

  async function handleDelete(file: FileInfo) {
    if (!deps.canWrite.value) return;
    // 删除不可恢复（版本历史随条目级联删除），必须二次确认
    const isDirectory = file.kind === "directory";
    const ok = await confirmDialog({
      title: isDirectory ? "删除目录" : "删除文件",
      message: isDirectory
        ? `确定要删除“${file.name}”吗？目录及其全部内容将被删除，且不可撤销。`
        : `确定要删除“${file.name}”吗？删除后不可撤销。`,
      confirmText: "删除",
      danger: true,
    });
    if (!ok) return;

    try {
      await deps.deletePath(
        file.path,
        `${isDirectory ? "删除目录" : "删除文件"}: ${file.path}`,
      );
      if (deps.desktopActivePath.value === file.path) {
        deps.desktopActivePath.value = "";
      }
      if (deps.searchActive.value) {
        await deps.doSearch(false);
      }
      deps.notifySuccess(`已删除「${file.name}」`);
    } catch (err) {
      deps.notifyError(err instanceof Error ? err.message : "删除失败");
    }
  }

  async function handleCreateDirectory(file: FileInfo) {
    if (!deps.canWrite.value || file.kind !== "directory") return;
    await deps.promptCreateDirectory(file.path);
  }

  async function createDirectoryHere() {
    if (!deps.canWrite.value) return;
    await deps.promptCreateDirectory(deps.currentPath.value || "");
  }

  function handleMoveEntry(file: FileInfo) {
    if (!deps.canWrite.value) return;
    deps.openMoveForEntry(file);
  }

  function handleViewHistory(file: FileInfo) {
    selectedFile.value = file;
    showHistory.value = true;
  }

  function handleShare(file: FileInfo) {
    if (!deps.canShare.value) return;
    selectedFile.value = file;
    showShareDialog.value = true;
  }

  function closeShareDialog() {
    showShareDialog.value = false;
  }

  function openDetailsDialog(file: FileInfo) {
    detailsFile.value = file;
    showDetailsDialog.value = true;
  }

  return {
    selectedFile,
    detailsDialogFile: detailsFile,
    showHistory,
    showShareDialog,
    showDetailsDialog,
    handleDelete,
    handleCreateDirectory,
    createDirectoryHere,
    handleMoveEntry,
    handleViewHistory,
    handleShare,
    closeShareDialog,
    openDetailsDialog,
  };
}
