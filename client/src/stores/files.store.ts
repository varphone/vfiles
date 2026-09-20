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
  // 服务端分页状态：total 为当前目录全量条目数，hasMoreFiles 表示还有未加载的页。
  const totalFiles = ref(0);
  const hasMoreFiles = ref(false);
  const loadingMoreFiles = ref(false);
  // 追加分页失败单独记录：已加载的条目不应因“加载更多”失败而整屏报错。
  const loadMoreError = ref<string | null>(null);
  const PAGE_SIZE = 200;

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
    loadingMoreFiles.value = false;
    loadMoreError.value = null;

    try {
      const page = await filesService.getFilesPage(path, {
        commit,
        limit: PAGE_SIZE,
        offset: 0,
      });
      if (requestId !== loadSequence) return;
      files.value = page.items;
      totalFiles.value = page.total;
      hasMoreFiles.value = page.has_more;
      currentPath.value = path;
    } catch (err) {
      if (requestId !== loadSequence) return;
      error.value = err instanceof Error ? err.message : "加载文件失败";
      files.value = [];
      totalFiles.value = 0;
      hasMoreFiles.value = false;
    } finally {
      if (requestId === loadSequence) loading.value = false;
    }
  }

  /** 加载下一页并追加到当前列表（目录未切换时才生效）。 */
  async function loadMoreFiles() {
    if (!hasMoreFiles.value || loadingMoreFiles.value) return;

    const requestId = loadSequence;
    const path = currentPath.value;
    loadingMoreFiles.value = true;
    loadMoreError.value = null;

    try {
      const page = await filesService.getFilesPage(path, {
        commit: browseCommit.value,
        limit: PAGE_SIZE,
        offset: files.value.length,
      });
      if (requestId !== loadSequence) return;
      files.value = [...files.value, ...page.items];
      totalFiles.value = page.total;
      hasMoreFiles.value = page.has_more;
    } catch (err) {
      if (requestId === loadSequence) {
        loadMoreError.value =
          err instanceof Error ? err.message : "加载更多失败";
      }
    } finally {
      if (requestId === loadSequence) loadingMoreFiles.value = false;
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
    totalFiles,
    hasMoreFiles,
    loadingMoreFiles,
    loadMoreError,
    breadcrumbs,
    directories,
    regularFiles,
    loadFiles,
    loadMoreFiles,
    setBrowseCommit,
    uploadFile,
    deleteFile,
    navigateTo,
    goBack,
  };
});
