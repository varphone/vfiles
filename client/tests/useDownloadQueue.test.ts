import { beforeEach, describe, expect, it, vi } from "vitest";
import { ref } from "vue";
import { useDownloadQueue } from "../src/composables/useDownloadQueue";

const {
  fetchFileDownloadMock,
  fetchFolderDownloadMock,
  saveDownloadedBlobMock,
} = vi.hoisted(() => ({
  fetchFileDownloadMock: vi.fn(),
  fetchFolderDownloadMock: vi.fn(),
  saveDownloadedBlobMock: vi.fn(),
}));

vi.mock("../src/services/files.service", () => ({
  filesService: {
    fetchFileDownload: fetchFileDownloadMock,
    fetchFolderDownload: fetchFolderDownloadMock,
    saveDownloadedBlob: saveDownloadedBlobMock,
  },
}));

function deferred<T>() {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((res) => {
    resolve = res;
  });
  return { promise, resolve };
}

const flush = () => new Promise((resolve) => setTimeout(resolve, 0));

describe("useDownloadQueue", () => {
  beforeEach(() => {
    fetchFileDownloadMock.mockReset();
    fetchFolderDownloadMock.mockReset();
    saveDownloadedBlobMock.mockReset();
  });

  it("derives the filename for files and folders", () => {
    const slow = deferred<{ blob: Blob; filename: string }>();
    fetchFileDownloadMock.mockReturnValue(slow.promise);
    fetchFolderDownloadMock.mockReturnValue(slow.promise);

    const queue = useDownloadQueue(ref(undefined));
    queue.enqueueDownload("file", "docs/readme.txt");
    queue.enqueueDownload("folder", "photos/2026");

    expect(queue.downloadQueue.value.map((item) => item.filename)).toEqual([
      "readme.txt",
      "2026.zip",
    ]);
  });

  it("processes downloads one at a time", async () => {
    const first = deferred<{ blob: Blob; filename: string }>();
    const second = deferred<{ blob: Blob; filename: string }>();
    fetchFileDownloadMock.mockReturnValueOnce(first.promise);
    fetchFileDownloadMock.mockReturnValueOnce(second.promise);

    const queue = useDownloadQueue(ref(undefined));
    queue.enqueueDownload("file", "a.txt");
    queue.enqueueDownload("file", "b.txt");

    await flush();
    expect(queue.downloadQueue.value[0].status).toBe("downloading");
    expect(queue.downloadQueue.value[1].status).toBe("queued");
    expect(fetchFileDownloadMock).toHaveBeenCalledTimes(1);

    // 第一个完成后才启动第二个
    first.resolve({ blob: new Blob(["ok"]), filename: "a.txt" });
    await flush();

    expect(queue.downloadQueue.value[0].status).toBe("done");
    expect(queue.downloadQueue.value[1].status).toBe("downloading");
    expect(fetchFileDownloadMock).toHaveBeenCalledTimes(2);
    expect(fetchFileDownloadMock.mock.calls[1][0]).toBe("b.txt");

    second.resolve({ blob: new Blob(["ok"]), filename: "b.txt" });
    await flush();
    expect(queue.downloadQueue.value[1].status).toBe("done");
    expect(saveDownloadedBlobMock).toHaveBeenCalledTimes(2);
  });

  it("cancels a queued item without downloading it", async () => {
    const first = deferred<{ blob: Blob; filename: string }>();
    fetchFileDownloadMock.mockReturnValueOnce(first.promise);

    const queue = useDownloadQueue(ref(undefined));
    queue.enqueueDownload("file", "a.txt");
    queue.enqueueDownload("file", "b.txt");
    await flush();

    queue.cancelItem(queue.downloadQueue.value[1].id);
    expect(queue.downloadQueue.value[1].status).toBe("canceled");

    first.resolve({ blob: new Blob(["ok"]), filename: "a.txt" });
    await flush();
    expect(fetchFileDownloadMock).toHaveBeenCalledTimes(1);
  });

  it("clears finished items but keeps active ones", async () => {
    fetchFileDownloadMock.mockResolvedValue({
      blob: new Blob(["ok"]),
      filename: "a.txt",
    });

    const queue = useDownloadQueue(ref(undefined));
    queue.enqueueDownload("file", "a.txt");
    await flush();
    expect(queue.downloadQueue.value[0].status).toBe("done");

    queue.clearFinished();
    expect(queue.downloadQueue.value).toHaveLength(0);
  });

  it("formats progress as percent and transferred size", () => {
    const queue = useDownloadQueue(ref(undefined));
    expect(queue.formatProgress(512, 1024)).toBe(" 50% (512.0 B/1.0 KB)");
  });

  it("retries a failed item and completes it", async () => {
    fetchFileDownloadMock.mockRejectedValueOnce(new Error("boom"));
    fetchFileDownloadMock.mockResolvedValueOnce({
      blob: new Blob(["ok"]),
      filename: "a.txt",
    });

    const queue = useDownloadQueue(ref(undefined));
    queue.enqueueDownload("file", "a.txt");
    await flush();
    expect(queue.downloadQueue.value[0].status).toBe("error");
    expect(queue.downloadQueue.value[0].error).toBe("boom");

    queue.retryItem(queue.downloadQueue.value[0].id);
    await flush();

    expect(queue.downloadQueue.value[0].status).toBe("done");
    expect(queue.downloadQueue.value[0].error).toBeUndefined();
    expect(fetchFileDownloadMock).toHaveBeenCalledTimes(2);
  });

  it("ignores retry for active items", async () => {
    const gate = deferred<{ blob: Blob; filename: string }>();
    fetchFileDownloadMock.mockReturnValueOnce(gate.promise);

    const queue = useDownloadQueue(ref(undefined));
    queue.enqueueDownload("file", "a.txt");
    await flush();
    expect(queue.downloadQueue.value[0].status).toBe("downloading");

    queue.retryItem(queue.downloadQueue.value[0].id);
    await flush();
    expect(queue.downloadQueue.value[0].status).toBe("downloading");
    expect(fetchFileDownloadMock).toHaveBeenCalledTimes(1);

    gate.resolve({ blob: new Blob(["ok"]), filename: "a.txt" });
    await flush();
  });
});
