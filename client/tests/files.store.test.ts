import { beforeEach, describe, expect, it, vi } from "vitest";
import { createPinia, setActivePinia } from "pinia";
import { useFilesStore } from "../src/stores/files.store";
import type { FileInfo } from "../src/types";

const { getFilesMock } = vi.hoisted(() => ({
  getFilesMock: vi.fn(async (): Promise<FileInfo[]> => []),
}));

vi.mock("../src/services/files.service", () => ({
  filesService: {
    getFiles: getFilesMock,
    uploadFile: vi.fn(),
    deleteFile: vi.fn(),
  },
}));

function file(name: string): FileInfo {
  return {
    id: name,
    name,
    path: name,
    kind: "file",
    created_at: "2026-01-01T00:00:00.000Z",
    updated_at: "2026-01-01T00:00:00.000Z",
  };
}

function deferred<T>() {
  let resolve!: (value: T) => void;
  let reject!: (reason?: unknown) => void;
  const promise = new Promise<T>((res, rej) => {
    resolve = res;
    reject = rej;
  });
  return { promise, resolve, reject };
}

describe("files store", () => {
  beforeEach(() => {
    getFilesMock.mockReset();
    setActivePinia(createPinia());
  });

  it("ignores a stale directory response that resolves after a newer one", async () => {
    const slow = deferred<FileInfo[]>();
    getFilesMock.mockReturnValueOnce(slow.promise);
    getFilesMock.mockResolvedValueOnce([file("b.txt")]);

    const store = useFilesStore();
    const first = store.loadFiles("");
    const second = store.loadFiles("b");
    await second;

    expect(store.currentPath).toBe("b");
    expect(store.files.map((f) => f.name)).toEqual(["b.txt"]);
    expect(store.loading).toBe(false);

    // 旧请求此时才返回，必须被丢弃
    slow.resolve([file("a.txt")]);
    await first;

    expect(store.currentPath).toBe("b");
    expect(store.files.map((f) => f.name)).toEqual(["b.txt"]);
    expect(store.loading).toBe(false);
  });

  it("keeps loading until the newest request settles", async () => {
    const slow = deferred<FileInfo[]>();
    getFilesMock.mockReturnValueOnce([file("old.txt")] as never);
    getFilesMock.mockReturnValueOnce(slow.promise);

    const store = useFilesStore();
    await store.loadFiles("");
    const pending = store.loadFiles("deep");

    expect(store.loading).toBe(true);
    slow.resolve([file("deep.txt")]);
    await pending;
    expect(store.loading).toBe(false);
    expect(store.files.map((f) => f.name)).toEqual(["deep.txt"]);
  });
});
