import { afterEach, describe, expect, it, vi } from "vitest";
import { filesService } from "../src/services/files.service";

afterEach(() => {
  vi.unstubAllGlobals();
});

describe("filesService.getFileContent preview limits", () => {
  it("requests a bounded byte range and cancels oversized previews from headers", async () => {
    let canceled = false;
    const body = new ReadableStream<Uint8Array>({
      start(controller) {
        controller.enqueue(new Uint8Array([1, 2, 3]));
      },
      cancel() {
        canceled = true;
      },
    });
    const fetchMock = vi.fn().mockResolvedValue(
      new Response(body, {
        status: 206,
        headers: { "content-range": "bytes 0-2/100", "content-length": "3" },
      }),
    );
    vi.stubGlobal("fetch", fetchMock);

    await expect(
      filesService.getFileContent("docs/large.txt", undefined, { maxBytes: 3 }),
    ).rejects.toThrow("文件超过预览上限（3 字节）");

    expect(fetchMock).toHaveBeenCalledWith(
      "/api/files/content?path=docs%2Flarge.txt",
      expect.objectContaining({
        credentials: "include",
        headers: { Range: "bytes=0-2" },
      }),
    );
    expect(canceled).toBe(true);
  });

  it("enforces the byte cap if the server ignores Range and omits lengths", async () => {
    let canceled = false;
    const body = new ReadableStream<Uint8Array>({
      start(controller) {
        controller.enqueue(new Uint8Array([1, 2, 3, 4]));
      },
      cancel() {
        canceled = true;
      },
    });
    vi.stubGlobal("fetch", vi.fn().mockResolvedValue(new Response(body)));

    await expect(
      filesService.getFileContent("docs/unbounded.txt", undefined, {
        maxBytes: 3,
      }),
    ).rejects.toThrow("文件超过预览上限（3 字节）");
    expect(canceled).toBe(true);
  });

  it("accepts files exactly at the limit and empty files", async () => {
    const fetchMock = vi
      .fn()
      .mockResolvedValueOnce(
        new Response("abc", {
          status: 206,
          headers: {
            "content-range": "bytes 0-2/3",
            "content-type": "text/plain",
          },
        }),
      )
      .mockResolvedValueOnce(
        new Response(null, {
          status: 416,
          headers: { "content-range": "bytes */0" },
        }),
      );
    vi.stubGlobal("fetch", fetchMock);

    const exact = await filesService.getFileContent("exact.txt", undefined, {
      maxBytes: 3,
    });
    const empty = await filesService.getFileContent("empty.txt", undefined, {
      maxBytes: 3,
    });

    await expect(exact.text()).resolves.toBe("abc");
    expect(empty.size).toBe(0);
  });
});
