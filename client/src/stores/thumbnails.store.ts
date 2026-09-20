import { defineStore } from "pinia";
import { reactive } from "vue";
import { filesService } from "../services/files.service";
import { isImageFile } from "../utils/filePresentation";
import type { FileInfo } from "../types";

export type ThumbnailStatus = "loading" | "ready" | "error";

export interface ThumbnailEntry {
  status: ThumbnailStatus;
  url?: string;
}

/** 同时进行的缩略图请求数上限，避免占满浏览器并发连接。 */
const MAX_CONCURRENCY = 4;
/** 缓存上限，超过后按进入顺序回收并释放 objectURL，防止内存泄漏。 */
const MAX_CACHE_ENTRIES = 160;
/** 超过该体积的图片不做缩略图（走占位图标），避免为预览下载超大文件。 */
const MAX_SOURCE_BYTES = 25 * 1024 * 1024;

interface ThumbnailTask {
  key: string;
  path: string;
  commit?: string;
}

export function thumbnailKey(path: string, commit?: string): string {
  return commit ? `${commit}:${path}` : path;
}

export function shouldThumbnail(file: FileInfo): boolean {
  if (!isImageFile(file)) return false;
  if ((file as { uiRole?: string }).uiRole) return false;
  if (
    typeof file.size_bytes === "number" &&
    file.size_bytes > MAX_SOURCE_BYTES
  ) {
    return false;
  }
  return true;
}

export const useThumbnailStore = defineStore("thumbnails", () => {
  const entries = reactive(new Map<string, ThumbnailEntry>());
  const queue: ThumbnailTask[] = [];
  const readyOrder: string[] = [];
  let active = 0;

  function get(path: string, commit?: string): ThumbnailEntry | undefined {
    return entries.get(thumbnailKey(path, commit));
  }

  function request(file: FileInfo, commit?: string) {
    if (!shouldThumbnail(file)) return;

    const key = thumbnailKey(file.path, commit);
    if (entries.has(key)) return;

    entries.set(key, { status: "loading" });
    queue.push({ key, path: file.path, commit });
    pump();
  }

  function pump() {
    while (active < MAX_CONCURRENCY && queue.length > 0) {
      const task = queue.shift();
      if (!task) break;
      active += 1;
      void run(task).finally(() => {
        active -= 1;
        pump();
      });
    }
  }

  async function run(task: ThumbnailTask) {
    try {
      const blob = await filesService.getFileContent(task.path, task.commit);
      const url = URL.createObjectURL(blob);
      entries.set(task.key, { status: "ready", url });
      readyOrder.push(task.key);
      evict();
    } catch {
      entries.set(task.key, { status: "error" });
    }
  }

  function evict() {
    while (readyOrder.length > MAX_CACHE_ENTRIES) {
      const key = readyOrder.shift();
      if (!key) break;
      const entry = entries.get(key);
      if (entry?.url) URL.revokeObjectURL(entry.url);
      entries.delete(key);
    }
  }

  function release(path: string, commit?: string) {
    const key = thumbnailKey(path, commit);
    const entry = entries.get(key);
    if (entry?.url) URL.revokeObjectURL(entry.url);
    entries.delete(key);
    const index = readyOrder.indexOf(key);
    if (index !== -1) readyOrder.splice(index, 1);
  }

  function reset() {
    for (const entry of entries.values()) {
      if (entry.url) URL.revokeObjectURL(entry.url);
    }
    entries.clear();
    readyOrder.length = 0;
    queue.length = 0;
  }

  return { entries, get, request, release, reset };
});
