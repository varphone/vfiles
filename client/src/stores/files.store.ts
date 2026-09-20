import { defineStore } from "pinia";
import { ref, computed } from "vue";
import { filesService } from "../services/files.service";
import type { FileInfo } from "../types";

export const useFilesStore = defineStore("files", () => {
  // 状态
  const files = ref<FileInfo[]>([]);
  const currentPath = ref<string>("");
  const loading = ref(false);
  const error = ref<string | null>(null);
  // 浏览目录所基于的提交（用于“历史版本浏览”）。undefined 表示当前 HEAD/worktree。
  const browseCommit = ref<string | undefined>(undefined);

  // 计算属性
  const breadcrumbs = computed(() => {
    if (!currentPath.value) return [{ name: "根目录", path: "" }];

    const parts = currentPath.value.split("/");
    const crumbs = [{ name: "根目录", path: "" }];

    let path = "";
    for (const part of parts) {
      path = path ? `${path}/${part}` : part;
      crumbs.push({ name: part, path });
    }

    return crumbs;
  });

  const directories = computed(() => {
    return files.value.filter((f) => f.kind === "directory");
  });

  const regularFiles = computed(() => {
    return files.value.filter((f) => f.kind === "file");
  });

  // 方法
  // 递增的请求序号：并发或快速切换目录时只接受最新一次请求的结果，
  // 避免先发出的旧请求后返回而覆盖新目录的数据（也避免 loading 被提前置否）。
  let loadSequence = 0;

  async function loadFiles(path: string = "") {
    const requestId = ++loadSequence;
    const commit = browseCommit.value;
    loading.value = true;
    error.value = null;

    try {
      const result = await filesService.getFiles(path, commit);
      if (requestId !== loadSequence) return;
      files.value = result;
      currentPath.value = path;
    } catch (err) {
      if (requestId !== loadSequence) return;
      error.value = err instanceof Error ? err.message : "加载文件失败";
      files.value = [];
    } finally {
      if (requestId === loadSequence) loading.value = false;
    }
  }

  function setBrowseCommit(commit?: string) {
    const next = commit && commit.trim() ? commit.trim() : undefined;
    browseCommit.value = next;
    void loadFiles(currentPath.value);
  }

  function ensureHeadForWrite() {
    if (browseCommit.value) browseCommit.value = undefined;
  }

  async function uploadFile(file: File, message?: string) {
    loading.value = true;
    error.value = null;

    try {
      ensureHeadForWrite();
      await filesService.uploadFile(file, currentPath.value, message);
      await loadFiles(currentPath.value);
    } catch (err) {
      error.value = err instanceof Error ? err.message : "上传失败";
      throw err;
    } finally {
      loading.value = false;
    }
  }

  async function deleteFile(path: string, message?: string) {
    loading.value = true;
    error.value = null;

    try {
      ensureHeadForWrite();
      await filesService.deleteFile(path, message);
      await loadFiles(currentPath.value);
    } catch (err) {
      error.value = err instanceof Error ? err.message : "删除失败";
      throw err;
    } finally {
      loading.value = false;
    }
  }

  function navigateTo(path: string) {
    loadFiles(path);
  }

  function goBack() {
    if (!currentPath.value) return;

    const parts = currentPath.value.split("/");
    parts.pop();
    navigateTo(parts.join("/"));
  }

  return {
    files,
    currentPath,
    loading,
    error,
    browseCommit,
    breadcrumbs,
    directories,
    regularFiles,
    loadFiles,
    setBrowseCommit,
    uploadFile,
    deleteFile,
    navigateTo,
    goBack,
  };
});
