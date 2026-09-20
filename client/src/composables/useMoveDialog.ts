import { ref, type Ref } from "vue";
import { useAppStore } from "../stores/app.store";
import { filesService } from "../services/files.service";
import {
  normalizeTargetDirectory,
  parentDirectoryPath,
  planMoveOperations,
} from "../utils/filePaths";
import type { FileInfo } from "../types";

export interface MoveDialogDeps {
  currentPath: Ref<string>;
  /** 移动完成后的刷新（含搜索态重新搜索）。 */
  refreshAfterMutation: () => Promise<void>;
  /** 移动后同步批量选择中的路径。 */
  replaceSelectedPath: (oldPath: string, newPath: string) => void;
  getActivePath: () => string;
  setActivePath: (path: string) => void;
  clearSelection: () => void;
}

/**
 * 移动对话框：打开/关闭状态与批量移动的执行。
 *
 * 从 FileBrowser 抽出；依赖（选择、高亮、刷新）通过参数注入。
 */
export function useMoveDialog(deps: MoveDialogDeps) {
  const appStore = useAppStore();

  const showMoveDialog = ref(false);
  const moveDialogItems = ref<FileInfo[]>([]);
  const moveDialogInitialPath = ref("");
  const moveDialogSubmitting = ref(false);

  function resetMoveDialogState() {
    showMoveDialog.value = false;
    moveDialogItems.value = [];
    moveDialogInitialPath.value = "";
    moveDialogSubmitting.value = false;
  }

  function openMoveDialog(items: FileInfo[], initialPath: string) {
    if (items.length === 0) return;
    moveDialogItems.value = items;
    moveDialogInitialPath.value = normalizeTargetDirectory(initialPath);
    showMoveDialog.value = true;
  }

  function closeMoveDialog() {
    if (moveDialogSubmitting.value) return;
    resetMoveDialogState();
  }

  /** 打开单个条目的移动对话框，默认目标为其所在目录。 */
  function openMoveForEntry(file: FileInfo) {
    openMoveDialog([file], parentDirectoryPath(file.path));
  }

  /**
   * 执行一次（批量）移动：校验目标重名、逐项移动并同步选择/高亮，最后刷新。
   * 返回是否成功；失败时已通过通知提示。
   */
  async function performMove(
    items: FileInfo[],
    targetDir: string,
  ): Promise<boolean> {
    if (items.length === 0) return false;

    const normalizedTargetDir = normalizeTargetDirectory(targetDir);

    try {
      const targetEntries = await filesService.getFiles(normalizedTargetDir);
      const operations = planMoveOperations(
        items,
        normalizedTargetDir,
        targetEntries,
      );

      for (const { file, to } of operations) {
        await filesService.movePath(
          file.path,
          to,
          `移动${file.kind === "directory" ? "目录" : "文件"}: ${file.path} -> ${to}`,
        );
        deps.replaceSelectedPath(file.path, to);
        if (deps.getActivePath() === file.path) {
          deps.setActivePath(
            parentDirectoryPath(to) === deps.currentPath.value ? to : "",
          );
        }
      }

      const successMessage =
        items.length === 1
          ? items[0]?.kind === "directory"
            ? "目录移动成功"
            : "文件移动成功"
          : `已移动 ${items.length} 个项目`;

      if (items.length > 1) {
        deps.clearSelection();
      }

      appStore.success(successMessage);
      await deps.refreshAfterMutation();
      return true;
    } catch (err) {
      appStore.error(err instanceof Error ? err.message : "移动失败");
      return false;
    }
  }

  async function submitMoveDialog(targetDir: string) {
    const items = moveDialogItems.value.slice();
    if (items.length === 0) return;

    moveDialogSubmitting.value = true;
    const ok = await performMove(items, targetDir);
    if (ok) {
      resetMoveDialogState();
    } else {
      moveDialogSubmitting.value = false;
    }
  }

  /** 拖放移动单个条目到目标目录（不打开对话框）。 */
  async function moveEntryToDirectory(
    file: FileInfo,
    targetDir: string,
  ): Promise<boolean> {
    return await performMove([file], targetDir);
  }

  return {
    showMoveDialog,
    moveDialogItems,
    moveDialogInitialPath,
    moveDialogSubmitting,
    resetMoveDialogState,
    openMoveDialog,
    closeMoveDialog,
    openMoveForEntry,
    submitMoveDialog,
    moveEntryToDirectory,
  };
}
