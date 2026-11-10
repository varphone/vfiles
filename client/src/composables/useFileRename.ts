import { ref, type Ref } from "vue";
import type { FileInfo } from "../types";
import { buildSiblingPath, isSafeDirName } from "../utils/filePaths";

export interface FileRenameDeps {
  canWrite: Ref<boolean>;
  desktopActivePath: Ref<string>;
  replaceSelectedPath: (oldPath: string, newPath: string) => void;
  renameEntryPath: (
    path: string,
    name: string,
    detail: string,
  ) => Promise<string>;
  refreshAfterMutation: () => void | Promise<void>;
  notifySuccess: (message: string) => void;
  notifyError: (message: string) => void;
}

/**
 * 行内外联重命名：进入编辑、校验名称、调用接口并刷新。
 *
 * 从 FileBrowser 抽出，重命名只依赖当前活动行、选择集更新和目录管理器的
 * 重命名接口，避免继续堆在主组件里。
 */
export function useFileRename(deps: FileRenameDeps) {
  const renamingPath = ref("");

  function normalizeEntryName(
    rawName: string,
    invalidMessage: string,
  ): string | null {
    const name = rawName.trim();
    if (!isSafeDirName(name)) {
      deps.notifyError(invalidMessage);
      return null;
    }
    return name;
  }

  function handleRenameEntry(file: FileInfo) {
    if (!deps.canWrite.value) return;
    deps.desktopActivePath.value = file.path;
    renamingPath.value = file.path;
  }

  async function commitRenameEntry(file: FileInfo, rawName: string) {
    renamingPath.value = "";
    if (!rawName) return;

    const name = normalizeEntryName(rawName, "非法名称");
    if (!name) return;
    if (name === file.name) {
      deps.notifyError("名称未变化");
      return;
    }

    try {
      const to = await deps.renameEntryPath(
        file.path,
        name,
        `重命名${file.kind === "directory" ? "目录" : "项目"}: ${file.path} -> ${buildSiblingPath(file.path, name)}`,
      );
      deps.replaceSelectedPath(file.path, to);
      if (deps.desktopActivePath.value === file.path) {
        deps.desktopActivePath.value = to;
      }
      deps.notifySuccess(
        `已重命名为「${to.split("/").filter(Boolean).pop() ?? to}」`,
      );
      await deps.refreshAfterMutation();
    } catch (err) {
      deps.notifyError(err instanceof Error ? err.message : "重命名失败");
    }
  }

  return {
    renamingPath,
    handleRenameEntry,
    commitRenameEntry,
    cancelRenameEntry: () => {
      renamingPath.value = "";
    },
  };
}
