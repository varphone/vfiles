import { afterEach, describe, expect, it, vi } from "vitest";
import { filesService } from "../src/services/files.service";

afterEach(() => {
  vi.unstubAllGlobals();
});

describe("filesService.fetchFileDownload", () => {
  it("reports progress while passing chunks through to the browser Blob reader", async () => {
    const body = new ReadableStream<Uint8Array>({
      start(controller) {
        controller.enqueue(new Uint8Array([1, 2]));
        controller.enqueue(new Uint8Array([3, 4, 5]));
        controller.close();
      },
    });
    vi.stubGlobal(
      "fetch",
      vi.fn().mockResolvedValue(
        new Response(body, {
          headers: {
            "content-length": "5",
            "content-type": "application/octet-stream",
          },
        }),
      ),
    );
    const onProgress = vi.fn();

    const result = await filesService.fetchFileDownload("large.bin", undefined, {
      onProgress,
    });

    expect(result.filename).toBe("large.bin");
    expect(result.blob.type).toBe("application/octet-stream");
    expect([...new Uint8Array(await result.blob.arrayBuffer())]).toEqual([
      1, 2, 3, 4, 5,
    ]);
    expect(onProgress.mock.calls.map(([progress]) => progress)).toEqual([
      { loaded: 2, total: 5 },
      { loaded: 5, total: 5 },
    ]);
  });

  it("cancels the response stream when the download signal is aborted", async () => {
    const controller = new AbortController();
    let notifyPull: () => void = () => undefined;
    const pullStarted = new Promise<void>((resolve) => {
      notifyPull = resolve;
    });
    let canceled = false;
    const body = new ReadableStream<Uint8Array>({
      pull() {
        notifyPull();
        return new Promise<void>(() => undefined);
      },
      cancel() {
        canceled = true;
      },
    });
    vi.stubGlobal("fetch", vi.fn().mockResolvedValue(new Response(body)));

    const pending = filesService.fetchFileDownload("large.bin", undefined, {
      signal: controller.signal,
      onProgress: vi.fn(),
    });
    await pullStarted;
    controller.abort();

    await expect(pending).rejects.toMatchObject({ name: "AbortError" });
    expect(canceled).toBe(true);
  });
});
