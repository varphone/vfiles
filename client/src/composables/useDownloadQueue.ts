import { computed, ref, type Ref } from "vue";
import { filesService } from "../services/files.service";
import { formatDownloadProgress } from "../utils/filePresentation";

export type DownloadQueueStatus =
  | "queued"
  | "downloading"
  | "done"
  | "error"
  | "canceled";

export type DownloadQueueKind = "file" | "folder";

export interface DownloadQueueItem {
  id: number;
  kind: DownloadQueueKind;
  path: string;
  filename: string;
  status: DownloadQueueStatus;
  progress?: { loaded: number; total?: number };
  error?: string;
  abort?: AbortController;
}

/**
 * 下载队列：串行执行，支持进度、取消与失败提示。
 *
 * 从 FileBrowser 中抽出，便于单测与复用；`browseCommit` 用于把下载固定到
 * 当前浏览的历史版本（为空表示 HEAD）。
 */
export function useDownloadQueue(browseCommit: Ref<string | undefined>) {
  const queueCollapsed = ref(false);
  const downloadQueue = ref<DownloadQueueItem[]>([]);
  let nextDownloadId = 1;

  const downloading = computed(() =>
    downloadQueue.value.some((item) => item.status === "downloading"),
  );

  const activeDownload = computed(() =>
    downloadQueue.value.find((item) => item.status === "downloading"),
  );

  const activeDownloadPercent = computed(() => {
    const active = activeDownload.value;
    if (!active?.progress?.total || active.progress.total <= 0) return null;
    return Math.min(
      100,
      Math.floor((active.progress.loaded / active.progress.total) * 100),
    );
  });

  function enqueueDownload(kind: DownloadQueueKind, path: string) {
    const wasEmpty = downloadQueue.value.length === 0;
    const filename =
      kind === "folder"
        ? `${path.split("/").filter(Boolean).pop() || "root"}.zip`
        : path.split("/").pop() || "download";

    downloadQueue.value = [
      ...downloadQueue.value,
      {
        id: nextDownloadId++,
        kind,
        path,
        filename,
        status: "queued",
      },
    ];

    // 第一次出现队列时默认展开，方便用户查看进度。
    if (wasEmpty) queueCollapsed.value = false;

    void processQueue();
  }

  function toggleQueuePanel() {
    queueCollapsed.value = !queueCollapsed.value;
  }

  async function processQueue() {
    if (downloading.value) return;

    const next = downloadQueue.value.find((item) => item.status === "queued");
    if (!next) return;

    const abort = new AbortController();
    downloadQueue.value = downloadQueue.value.map((item) =>
      item.id === next.id
        ? { ...item, status: "downloading", progress: { loaded: 0 }, abort }
        : item,
    );

    try {
      const onProgress = (progress: { loaded: number; total?: number }) => {
        downloadQueue.value = downloadQueue.value.map((item) =>
          item.id === next.id ? { ...item, progress } : item,
        );
      };

      const commit = browseCommit.value;
      const result =
        next.kind === "folder"
          ? await filesService.fetchFolderDownload(next.path, commit, {
              signal: abort.signal,
              onProgress,
            })
          : await filesService.fetchFileDownload(next.path, commit, {
              signal: abort.signal,
              onProgress,
            });

      filesService.saveDownloadedBlob(result.blob, result.filename);
      downloadQueue.value = downloadQueue.value.map((item) =>
        item.id === next.id
          ? { ...item, status: "done", abort: undefined }
          : item,
      );
    } catch (err) {
      const isAbort = (err as { name?: string } | null)?.name === "AbortError";
      downloadQueue.value = downloadQueue.value.map((item) =>
        item.id === next.id
          ? {
              ...item,
              status: isAbort ? "canceled" : "error",
              error: isAbort
                ? undefined
                : err instanceof Error
                  ? err.message
                  : "下载失败",
              abort: undefined,
            }
          : item,
      );
    } finally {
      // 继续下一个
      void processQueue();
    }
  }

  function formatProgress(loaded: number, total: number): string {
    return formatDownloadProgress(loaded, total);
  }

  function cancelItem(id: number) {
    const item = downloadQueue.value.find((entry) => entry.id === id);
    if (!item) return;

    if (item.status === "queued") {
      downloadQueue.value = downloadQueue.value.map((entry) =>
        entry.id === id ? { ...entry, status: "canceled" } : entry,
      );
      return;
    }

    if (item.status === "downloading") {
      item.abort?.abort();
    }
  }

  function cancelAll() {
    for (const item of downloadQueue.value) {
      if (item.status === "queued") {
        downloadQueue.value = downloadQueue.value.map((entry) =>
          entry.id === item.id ? { ...entry, status: "canceled" } : entry,
        );
      } else if (item.status === "downloading") {
        item.abort?.abort();
      }
    }
  }

  function clearFinished() {
    downloadQueue.value = downloadQueue.value.filter(
      (item) => item.status === "queued" || item.status === "downloading",
    );
  }

  function removeItem(id: number) {
    downloadQueue.value = downloadQueue.value.filter((item) => item.id !== id);
  }

  /** 重试失败或已取消的条目：重新排队并继续处理。 */
  function retryItem(id: number) {
    const item = downloadQueue.value.find((entry) => entry.id === id);
    if (!item) return;
    if (item.status === "queued" || item.status === "downloading") return;

    downloadQueue.value = downloadQueue.value.map((entry) =>
      entry.id === id
        ? {
            ...entry,
            status: "queued",
            error: undefined,
            progress: undefined,
            abort: undefined,
          }
        : entry,
    );
    void processQueue();
  }

  return {
    queueCollapsed,
    downloadQueue,
    downloading,
    activeDownload,
    activeDownloadPercent,
    enqueueDownload,
    toggleQueuePanel,
    processQueue,
    formatProgress,
    cancelItem,
    cancelAll,
    clearFinished,
    removeItem,
    retryItem,
  };
}
