import { beforeEach, describe, expect, it, vi } from "vitest";
import { createPinia, setActivePinia } from "pinia";
import { useFilesStore } from "../src/stores/files.store";
import type { FileInfo } from "../src/types";

type PageOpts = { commit?: string; limit?: number; offset?: number };
type PageResult = {
  items: FileInfo[];
  total: number;
  limit: number;
  offset: number;
  has_more: boolean;
};

const { getFilesMock, getFilesPageMock } = vi.hoisted(() => ({
  getFilesMock: vi.fn(
    async (_path: string, _commit?: string): Promise<FileInfo[]> => [],
  ),
  getFilesPageMock:
    vi.fn<(path: string, opts?: PageOpts) => Promise<PageResult>>(),
}));

// 服务端分页：测试里以 getFilesMock 作为“全量数据源”，getFilesPage 据此切片，
// 这样既保留原有断言（调用次数、延迟、拒绝），又覆盖分页字段。
getFilesPageMock.mockImplementation(
  async (
    path: string,
    opts?: { commit?: string; limit?: number; offset?: number },
  ) => {
    const all = await getFilesMock(path, opts?.commit);
    const offset = opts?.offset ?? 0;
    const limit = opts?.limit ?? all.length;
    const items = all.slice(offset, offset + limit);
    return {
      items,
      total: all.length,
      limit,
      offset,
      has_more: offset + items.length < all.length,
    };
  },
);

vi.mock("../src/services/files.service", () => ({
  filesService: {
    getFiles: getFilesMock,
    getFilesPage: getFilesPageMock,
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
    getFilesPageMock.mockClear();
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

  it("exposes total/hasMore from the first page and appends the next page", async () => {
    const all = Array.from({ length: 450 }, (_, i) => file(`f${i}.txt`));
    getFilesMock.mockImplementation(async () => all);

    const store = useFilesStore();
    await store.loadFiles("");

    expect(store.files).toHaveLength(200);
    expect(store.totalFiles).toBe(450);
    expect(store.hasMoreFiles).toBe(true);
    expect(getFilesPageMock).toHaveBeenLastCalledWith("", {
      commit: undefined,
      limit: 200,
      offset: 0,
    });

    await store.loadMoreFiles();

    expect(store.files).toHaveLength(400);
    expect(store.files[399].name).toBe("f399.txt");
    expect(store.hasMoreFiles).toBe(true);
    expect(store.loadingMoreFiles).toBe(false);
    expect(getFilesPageMock).toHaveBeenLastCalledWith("", {
      commit: undefined,
      limit: 200,
      offset: 200,
    });

    await store.loadMoreFiles();

    expect(store.files).toHaveLength(450);
    expect(store.hasMoreFiles).toBe(false);
  });

  it("drops a page that resolves after the directory changed", async () => {
    const all = Array.from({ length: 450 }, (_, i) => file(`f${i}.txt`));
    getFilesMock.mockImplementation(async () => all);

    const store = useFilesStore();
    await store.loadFiles("");

    const slow = deferred<FileInfo[]>();
    getFilesMock.mockReturnValueOnce(slow.promise);
    const pending = store.loadMoreFiles();

    // 用户切到别的目录，旧目录的第二页随后才返回
    getFilesMock.mockImplementation(async () => [file("other.txt")]);
    await store.loadFiles("other");

    slow.resolve(all);
    await pending;

    expect(store.currentPath).toBe("other");
    expect(store.files.map((f) => f.name)).toEqual(["other.txt"]);
    expect(store.loadingMoreFiles).toBe(false);
  });

  it("keeps loaded items when appending a page fails", async () => {
    const all = Array.from({ length: 450 }, (_, i) => file(`f${i}.txt`));
    getFilesMock.mockImplementation(async () => all);

    const store = useFilesStore();
    await store.loadFiles("");

    getFilesMock.mockRejectedValueOnce(new Error("网络中断"));
    await store.loadMoreFiles();

    // 已加载的第一页仍可用，只有“加载更多”区域报错。
    expect(store.files).toHaveLength(200);
    expect(store.error).toBeNull();
    expect(store.loadMoreError).toBe("网络中断");
    expect(store.loadingMoreFiles).toBe(false);
    expect(store.hasMoreFiles).toBe(true);

    await store.loadMoreFiles();
    expect(store.loadMoreError).toBeNull();
    expect(store.files).toHaveLength(400);
  });

  it("does not request a page when there is nothing more to load", async () => {
    getFilesMock.mockResolvedValueOnce([file("only.txt")] as never);

    const store = useFilesStore();
    await store.loadFiles("");

    expect(store.hasMoreFiles).toBe(false);
    getFilesPageMock.mockClear();

    await store.loadMoreFiles();

    expect(getFilesPageMock).not.toHaveBeenCalled();
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
