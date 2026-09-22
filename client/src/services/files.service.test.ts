import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const { getMock, postMock, putBinaryWithProgressMock, postFormNativeMock } =
  vi.hoisted(() => ({
    getMock: vi.fn(),
    postMock: vi.fn(),
    putBinaryWithProgressMock: vi.fn(),
    postFormNativeMock: vi.fn(),
  }));

vi.mock("./api.service", () => ({
  apiService: {
    get: getMock,
    post: postMock,
    putBinaryWithProgress: putBinaryWithProgressMock,
    postFormNative: postFormNativeMock,
  },
}));

import { filesService } from "./files.service";

describe("filesService.createShareLink", () => {
  beforeEach(() => {
    getMock.mockReset();
    postMock.mockReset();
    putBinaryWithProgressMock.mockReset();
    postFormNativeMock.mockReset();
  });

  afterEach(() => {
    vi.unstubAllGlobals();
  });

  it("uses the current Rust share API contract", async () => {
    postMock.mockResolvedValue({
      code: "abc12345",
      share_url: "http://localhost:3000/s/abc12345",
    });

    const result = await filesService.createShareLink("docs/readme.md", {
      commit: "0123456",
      ttl: 3600,
    });

    expect(postMock).toHaveBeenCalledWith("/share/shares", {
      path: "docs/readme.md",
      expires_at: expect.any(String),
    });
    expect(result).toEqual({
      code: "abc12345",
      url: "http://localhost:3000/s/abc12345",
      expiresIn: 3600,
      expiresAt: expect.any(String),
    });
  });

  it("maps Rust search results to flat file items", async () => {
    getMock.mockResolvedValue([
      {
        entry: {
          id: "entry-1",
          name: "2.4.3a0.tgz",
          path: "packages/2.4.3a0.tgz",
          kind: "file",
          created_at: "2026-05-28T00:00:00Z",
        },
        version: {
          size_bytes: 123,
          mime_type: "application/gzip",
          is_text: false,
          created_at: "2026-05-28T01:00:00Z",
        },
        matches: [],
        score: 1,
      },
    ]);

    const result = await filesService.searchFiles("2.4");

    expect(getMock).toHaveBeenCalledWith("/files/search", {
      q: "2.4",
      search_files: true,
      search_content: false,
      limit: 500,
      offset: 0,
    });
    expect(result).toEqual([
      {
        id: "entry-1",
        name: "2.4.3a0.tgz",
        path: "packages/2.4.3a0.tgz",
        kind: "file",
        created_at: "2026-05-28T00:00:00Z",
        updated_at: "2026-05-28T01:00:00Z",
        size_bytes: 123,
        mime_type: "application/gzip",
        is_text: false,
        matches: [],
      },
    ]);
  });

  it("merges duplicate Rust search results and keeps scoped content matches", async () => {
    getMock.mockResolvedValue([
      {
        entry: {
          id: "entry-1",
          name: "2.4.3a0.tgz",
          path: "packages/2.4.3a0.tgz",
          kind: "file",
          created_at: "2026-05-28T00:00:00Z",
        },
        version: {
          size_bytes: 123,
          mime_type: "application/gzip",
          is_text: false,
          created_at: "2026-05-28T01:00:00Z",
        },
        matches: [],
        score: 1,
      },
      {
        entry: {
          id: "entry-1",
          name: "2.4.3a0.tgz",
          path: "packages/2.4.3a0.tgz",
          kind: "file",
          created_at: "2026-05-28T00:00:00Z",
        },
        version: {
          size_bytes: 123,
          mime_type: "application/gzip",
          is_text: false,
          created_at: "2026-05-28T01:00:00Z",
        },
        matches: [
          {
            match_type: "content",
            context: "artifact 2.4 release",
            line_number: 4,
          },
          {
            match_type: "content",
            context: "artifact 2.4 release",
            line_number: 4,
          },
        ],
        score: 0.8,
      },
    ]);

    const result = await filesService.searchFiles("2.4", "content", {
      path: "packages",
      type: "file",
    });

    expect(getMock).toHaveBeenCalledWith("/files/search", {
      q: "2.4",
      search_files: true,
      search_content: true,
      limit: 500,
      offset: 0,
      path: "packages",
      type: "file",
    });

    expect(result).toEqual([
      {
        id: "entry-1",
        name: "2.4.3a0.tgz",
        path: "packages/2.4.3a0.tgz",
        kind: "file",
        created_at: "2026-05-28T00:00:00Z",
        updated_at: "2026-05-28T01:00:00Z",
        size_bytes: 123,
        mime_type: "application/gzip",
        is_text: false,
        matches: [{ line: 4, text: "artifact 2.4 release" }],
      },
    ]);
  });

  it("falls back to native upload when Firefox blocks chunk reads", async () => {
    postMock.mockResolvedValueOnce({
      upload_id: "upload-1",
      chunk_size: 4,
      total_chunks: 1,
    });
    postFormNativeMock.mockResolvedValue({ completed: true });

    class MockFileReader {
      result: ArrayBuffer | string | null = null;
      error: DOMException | null = new DOMException(
        "Firefox blocked file access",
        "AbortError",
      );
      onload: null | (() => void) = null;
      onerror: null | (() => void) = null;
      onabort: null | (() => void) = null;

      readAsArrayBuffer() {
        queueMicrotask(() => {
          this.onabort?.();
        });
      }

      abort() {
        queueMicrotask(() => {
          this.onabort?.();
        });
      }
    }

    vi.stubGlobal("FileReader", MockFileReader as any);

    await expect(
      filesService.uploadFile(
        new File(["data"], "problematic.tar.gz"),
        "",
        "上传文件",
      ),
    ).resolves.toEqual({ completed: true });

    expect(putBinaryWithProgressMock).not.toHaveBeenCalled();
    expect(postFormNativeMock).toHaveBeenCalledTimes(1);
    const [url, formData] = postFormNativeMock.mock.calls[0];
    expect(url).toBe("/files/upload");
    expect(formData).toBeInstanceOf(FormData);
    expect(formData.get("file")).toBeInstanceOf(File);
  });

  it("preserves the Firefox read error if native fallback also fails generically", async () => {
    postMock.mockResolvedValueOnce({
      upload_id: "upload-1",
      chunk_size: 4,
      total_chunks: 1,
    });
    postFormNativeMock.mockRejectedValue(new Error("网络错误，请检查连接"));

    class MockFileReader {
      result: ArrayBuffer | string | null = null;
      error: DOMException | null = new DOMException(
        "Firefox blocked file access",
        "AbortError",
      );
      onload: null | (() => void) = null;
      onerror: null | (() => void) = null;
      onabort: null | (() => void) = null;

      readAsArrayBuffer() {
        queueMicrotask(() => {
          this.onabort?.();
        });
      }

      abort() {
        queueMicrotask(() => {
          this.onabort?.();
        });
      }
    }

    vi.stubGlobal("FileReader", MockFileReader as any);

    await expect(
      filesService.uploadFile(
        new File(["data"], "problematic.tar.gz"),
        "",
        "上传文件",
      ),
    ).rejects.toThrow(/Firefox/);
  });

  it("advances progress after each uploaded chunk without xhr progress events", async () => {
    postMock
      .mockResolvedValueOnce({
        upload_id: "upload-1",
        chunk_size: 2,
        total_chunks: 2,
      })
      .mockResolvedValueOnce({
        completed: true,
      });
    putBinaryWithProgressMock.mockImplementation(
      async (
        _url: string,
        _data: ArrayBuffer,
        opts?: {
          onUploadProgress?: (p: { loaded: number; total?: number }) => void;
        },
      ) => {
        opts?.onUploadProgress?.({ loaded: 2, total: 2 });
        return { received: true };
      },
    );

    const loadedValues: number[] = [];

    await filesService.uploadFile(
      new File(["data"], "archive.tar.gz"),
      "",
      "上传文件",
      {
        onProgress: ({ loaded }) => {
          loadedValues.push(loaded);
        },
      },
    );

    expect(putBinaryWithProgressMock).toHaveBeenCalledTimes(2);
    expect(loadedValues).toEqual([0, 2, 2, 4, 4]);
  });

  it("slices chunks using the init response chunk size", async () => {
    postMock
      .mockResolvedValueOnce({
        upload_id: "upload-1",
        chunk_size: 3,
        total_chunks: 2,
      })
      .mockResolvedValueOnce({
        completed: true,
      });
    putBinaryWithProgressMock.mockResolvedValue({ received: true });

    await filesService.uploadFile(
      new File([new Uint8Array([1, 2, 3, 4, 5])], "chunked.bin"),
      "",
      "上传文件",
    );

    expect(putBinaryWithProgressMock).toHaveBeenCalledTimes(2);
    expect(
      (putBinaryWithProgressMock.mock.calls[0]?.[1] as ArrayBuffer).byteLength,
    ).toBe(3);
    expect(
      (putBinaryWithProgressMock.mock.calls[1]?.[1] as ArrayBuffer).byteLength,
    ).toBe(2);
  });
});
