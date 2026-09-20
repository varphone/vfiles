import { computed, ref, watch, type Ref } from "vue";
import { useAppStore } from "../stores/app.store";
import { filesService } from "../services/files.service";
import { promptDialog } from "./dialog";
import {
  buildChildPath,
  buildSiblingPath,
  isSafeDirName,
  parentDirectoryPath,
} from "../utils/filePaths";

export interface DirectoryManagerDeps {
  /** 当前浏览目录。 */
  currentPath: Ref<string>;
  /** 是否处于搜索结果视图（变更后需要重新执行搜索）。 */
  searchActive: Ref<boolean>;
  clearSearch: () => void;
  navigateTo: (path: string) => void;
  refresh: () => void | Promise<void>;
  doSearch: (pushHistoryEnabled: boolean) => Promise<void> | void;
  setActivePath: (path: string) => void;
}

/**
 * 目录管理：新建子目录、重命名/删除当前目录，以及目录操作后的刷新。
 *
 * 从 FileBrowser 抽出，依赖通过参数注入，便于测试与复用。
 */
export function useDirectoryManager(deps: DirectoryManagerDeps) {
  const appStore = useAppStore();

  const dirManagerOpen = ref(false);
  const dirOpLoading = ref<null | "create" | "rename" | "delete">(null);
  const dirOpBusy = computed(() => dirOpLoading.value !== null);
  const newDirName = ref("");
  const renameDirName = ref("");

  const currentDirName = computed(() => {
    if (!deps.currentPath.value) return "";
    const parts = deps.currentPath.value.split("/").filter(Boolean);
    return parts[parts.length - 1] || "";
  });

  watch(
    () => deps.currentPath.value,
    () => {
      renameDirName.value = currentDirName.value;
    },
    { immediate: true },
  );

  function normalizeEntryName(
    rawName: string,
    invalidMessage: string,
  ): string | null {
    const name = rawName.trim();
    if (!isSafeDirName(name)) {
      appStore.error(invalidMessage);
      return null;
    }
    return name;
  }

  async function refreshAfterMutation() {
    await deps.refresh();
    if (deps.searchActive.value) {
      await deps.doSearch(false);
    }
  }

  async function createDirectoryAt(
    parentPath: string,
    name: string,
  ): Promise<string> {
    const dirPath = buildChildPath(parentPath, name);
    await filesService.createDirectory(dirPath, `创建目录: ${dirPath}`);
    return dirPath;
  }

  async function renameEntryPath(
    path: string,
    name: string,
    message: string,
  ): Promise<string> {
    const targetPath = buildSiblingPath(path, name);
    await filesService.movePath(path, targetPath, message);
    return targetPath;
  }

  async function promptCreateDirectory(
    parentPath: string = deps.currentPath.value,
  ) {
    const raw = await promptDialog({
      title: "新建目录",
      message: "输入目录名（仅名称，不含路径分隔符）",
      placeholder: "目录名",
    });
    if (raw == null) return;

    const name = normalizeEntryName(raw, "非法目录名");
    if (!name) return;

    try {
      const dirPath = await createDirectoryAt(parentPath, name);
      appStore.success("目录创建成功");
      if (parentPath === deps.currentPath.value) {
        deps.setActivePath(dirPath);
      } else {
        if (deps.searchActive.value) {
          deps.clearSearch();
        }
        deps.navigateTo(parentPath);
        return;
      }
      await refreshAfterMutation();
    } catch (err) {
      appStore.error(err instanceof Error ? err.message : "目录创建失败");
    }
  }

  async function createSubDir() {
    const name = normalizeEntryName(newDirName.value, "非法目录名");
    if (!name) return;

    dirOpLoading.value = "create";
    try {
      const dirPath = await createDirectoryAt(deps.currentPath.value, name);
      appStore.success("目录创建成功");
      newDirName.value = "";
      deps.setActivePath(dirPath);
      await refreshAfterMutation();
    } catch (err) {
      appStore.error(err instanceof Error ? err.message : "目录创建失败");
    } finally {
      if (dirOpLoading.value === "create") dirOpLoading.value = null;
    }
  }

  async function renameCurrentDir() {
    if (!deps.currentPath.value) return;
    const name = normalizeEntryName(renameDirName.value, "非法目录名");
    if (!name) return;
    if (name === currentDirName.value) {
      appStore.error("目录名未变化");
      return;
    }

    dirOpLoading.value = "rename";
    const targetPath = buildSiblingPath(deps.currentPath.value, name);
    try {
      const to = await renameEntryPath(
        deps.currentPath.value,
        name,
        `重命名目录: ${deps.currentPath.value} -> ${targetPath}`,
      );
      appStore.success("重命名成功");
      dirManagerOpen.value = false;
      deps.navigateTo(to);
    } catch (err) {
      appStore.error(err instanceof Error ? err.message : "重命名失败");
    } finally {
      if (dirOpLoading.value === "rename") dirOpLoading.value = null;
    }
  }

  async function deleteCurrentDir() {
    if (!deps.currentPath.value) return;

    const expected = currentDirName.value;
    const typed = await promptDialog({
      title: "删除目录",
      message: `危险操作：删除目录 /${deps.currentPath.value}\n\n此操作会删除其下全部内容，并生成提交。\n请输入目录名“${expected}”以确认：`,
      placeholder: expected,
      confirmText: "删除",
      danger: true,
    });
    if (typed == null) return;
    if (typed.trim() !== expected) {
      appStore.error("确认失败：目录名不匹配");
      return;
    }

    const parent = parentDirectoryPath(deps.currentPath.value);

    dirOpLoading.value = "delete";
    try {
      await filesService.deleteFile(
        deps.currentPath.value,
        `删除目录: ${deps.currentPath.value}`,
      );
      appStore.success("目录删除成功");
      dirManagerOpen.value = false;
      deps.navigateTo(parent);
    } catch (err) {
      appStore.error(err instanceof Error ? err.message : "删除失败");
    } finally {
      if (dirOpLoading.value === "delete") dirOpLoading.value = null;
    }
  }

  return {
    dirManagerOpen,
    dirOpLoading,
    dirOpBusy,
    newDirName,
    renameDirName,
    currentDirName,
    refreshAfterMutation,
    createDirectoryAt,
    renameEntryPath,
    promptCreateDirectory,
    createSubDir,
    renameCurrentDir,
    deleteCurrentDir,
  };
}
