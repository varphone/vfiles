import { beforeEach, describe, expect, it, vi } from "vitest";
import { ref } from "vue";
import { useFileSearch } from "../src/composables/useFileSearch";

const { searchFilesMock, searchFilesPageMock } = vi.hoisted(() => {
  const searchFilesMock = vi.fn(async () => []);
  return {
    searchFilesMock,
    // 组合式函数读取分页信封，这里把「数据源 mock」包一层
    searchFilesPageMock: vi.fn(async (...args: unknown[]) => {
      const items = await (
        searchFilesMock as (...a: unknown[]) => Promise<unknown[]>
      )(...args);
      return {
        items,
        hasMore: false,
        limit: 100,
        offset: 0,
      };
    }),
  };
});

vi.mock("../src/services/files.service", () => ({
  SEARCH_PAGE_SIZE: 100,
  filesService: { searchFiles: searchFilesPageMock },
}));

function fileInfo(name: string) {
  return {
    id: name,
    name,
    path: name,
    kind: "file" as const,
    created_at: "2026-01-01T00:00:00.000Z",
  };
}

function installLocalStorageStub() {
  const backing = new Map<string, string>();
  const stub = {
    getItem: (key: string) => (backing.has(key) ? backing.get(key)! : null),
    setItem: (key: string, value: string) => {
      backing.set(key, String(value));
    },
    removeItem: (key: string) => {
      backing.delete(key);
    },
    clear: () => backing.clear(),
    key: (index: number) => Array.from(backing.keys())[index] ?? null,
    get length() {
      return backing.size;
    },
  };
  Object.defineProperty(globalThis, "localStorage", {
    value: stub,
    configurable: true,
    writable: true,
  });
}

describe("useFileSearch", () => {
  beforeEach(() => {
    installLocalStorageStub();
    searchFilesMock.mockReset();
    searchFilesMock.mockResolvedValue([]);
    searchFilesPageMock.mockClear();
  });

  it("deduplicates search history case-insensitively and caps it", () => {
    const search = useFileSearch(ref(""));
    for (let index = 0; index < 12; index += 1) {
      search.pushSearchHistory(`term-${index}`);
    }
    search.pushSearchHistory("TERM-5");

    expect(search.searchHistory.value).toHaveLength(10);
    expect(search.searchHistory.value[0]).toBe("TERM-5");
    expect(
      search.searchHistory.value.filter(
        (term) => term.toLowerCase() === "term-5",
      ),
    ).toHaveLength(1);
    expect(
      JSON.parse(localStorage.getItem("vfiles.searchHistory") || "[]"),
    ).toHaveLength(10);
  });

  it("scopes the query to the current directory when enabled", async () => {
    const search = useFileSearch(ref("docs/nested"));

    await search.doSearch(false);
    expect(searchFilesPageMock).toHaveBeenCalledTimes(0);

    search.searchQuery.value = "readme";
    search.searchScopeCurrent.value = true;
    await search.doSearch(false);

    expect(searchFilesPageMock).toHaveBeenCalledWith("readme", "name", {
      type: "all",
      path: "docs/nested",
      limit: 100,
      offset: 0,
    });

    search.searchScopeCurrent.value = false;
    await search.doSearch(false);
    expect(searchFilesPageMock).toHaveBeenLastCalledWith("readme", "name", {
      type: "all",
      path: "",
      limit: 100,
      offset: 0,
    });
  });

  it("appends the next page and tracks has_more", async () => {
    const search = useFileSearch(ref(""));
    search.searchQuery.value = "report";

    searchFilesPageMock.mockResolvedValueOnce({
      items: [fileInfo("report-1.txt"), fileInfo("report-2.txt")],
      hasMore: true,
      limit: 100,
      offset: 0,
    });
    await search.runSearch();

    expect(search.searchResults.value).toHaveLength(2);
    expect(search.searchHasMore.value).toBe(true);

    searchFilesPageMock.mockResolvedValueOnce({
      items: [fileInfo("report-3.txt")],
      hasMore: false,
      limit: 100,
      offset: 2,
    });
    await search.loadMoreSearchResults();

    expect(search.searchResults.value.map((item) => item.name)).toEqual([
      "report-1.txt",
      "report-2.txt",
      "report-3.txt",
    ]);
    expect(search.searchHasMore.value).toBe(false);
    expect(search.searchLoadingMore.value).toBe(false);
    // 第二页从已加载条数继续
    expect(searchFilesPageMock).toHaveBeenLastCalledWith("report", "name", {
      type: "all",
      path: "",
      limit: 100,
      offset: 2,
    });
  });

  it("does not request another page without more results", async () => {
    const search = useFileSearch(ref(""));
    search.searchQuery.value = "report";
    searchFilesPageMock.mockResolvedValueOnce({
      items: [fileInfo("report-1.txt")],
      hasMore: false,
      limit: 100,
      offset: 0,
    });
    await search.runSearch();
    searchFilesPageMock.mockClear();

    await search.loadMoreSearchResults();

    expect(searchFilesPageMock).not.toHaveBeenCalled();
  });

  it("drops a page that resolves after the query changed", async () => {
    const search = useFileSearch(ref(""));
    search.searchQuery.value = "first";
    searchFilesPageMock.mockResolvedValueOnce({
      items: [fileInfo("first-1.txt")],
      hasMore: true,
      limit: 100,
      offset: 0,
    });
    await search.runSearch();

    // 第二页在途时用户改了关键字并重新搜索
    let resolveSlow: (value: unknown) => void = () => {};
    searchFilesPageMock.mockReturnValueOnce(
      new Promise((resolve) => {
        resolveSlow = resolve;
      }) as never,
    );
    const pending = search.loadMoreSearchResults();

    search.searchQuery.value = "second";
    searchFilesPageMock.mockResolvedValueOnce({
      items: [fileInfo("second-1.txt")],
      hasMore: false,
      limit: 100,
      offset: 0,
    });
    await search.runSearch();

    resolveSlow({
      items: [fileInfo("first-2.txt")],
      hasMore: true,
      limit: 100,
      offset: 1,
    });
    await pending;

    expect(search.searchResults.value.map((item) => item.name)).toEqual([
      "second-1.txt",
    ]);
    expect(search.searchHasMore.value).toBe(false);
  });

  it("clears paging state when the search is cleared", async () => {
    const search = useFileSearch(ref(""));
    search.searchQuery.value = "report";
    searchFilesPageMock.mockResolvedValueOnce({
      items: [fileInfo("report-1.txt")],
      hasMore: true,
      limit: 100,
      offset: 0,
    });
    await search.runSearch();
    expect(search.searchHasMore.value).toBe(true);

    search.clearSearch();

    expect(search.searchHasMore.value).toBe(false);
    expect(search.searchLoadingMore.value).toBe(false);
  });

  it("clears query, results and error state", () => {
    const search = useFileSearch(ref(""));
    search.searchQuery.value = "abc";
    search.searchResults.value = [
      {
        id: "a",
        name: "a.txt",
        path: "a.txt",
        kind: "file",
        created_at: "2026-01-01T00:00:00.000Z",
      },
    ];
    search.searchError.value = "boom";
    search.searchActive.value = true;

    search.clearSearch();

    expect(search.searchQuery.value).toBe("");
    expect(search.searchResults.value).toEqual([]);
    expect(search.searchError.value).toBeNull();
    expect(search.searchActive.value).toBe(false);
  });
});
