import { beforeEach, describe, expect, it, vi } from "vitest";
import { ref } from "vue";
import { useFileSearch } from "../src/composables/useFileSearch";

const { searchFilesMock } = vi.hoisted(() => ({
  searchFilesMock: vi.fn(async () => []),
}));

vi.mock("../src/services/files.service", () => ({
  filesService: { searchFiles: searchFilesMock },
}));

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
    expect(searchFilesMock).toHaveBeenCalledTimes(0);

    search.searchQuery.value = "readme";
    search.searchScopeCurrent.value = true;
    await search.doSearch(false);

    expect(searchFilesMock).toHaveBeenCalledWith("readme", "name", {
      type: "all",
      path: "docs/nested",
    });

    search.searchScopeCurrent.value = false;
    await search.doSearch(false);
    expect(searchFilesMock).toHaveBeenLastCalledWith("readme", "name", {
      type: "all",
      path: "",
    });
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
