import { waitFor } from "@testing-library/vue";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { createPinia, setActivePinia } from "pinia";
import FileBrowser from "../src/components/file-browser/FileBrowser.vue";
import { useAppStore } from "../src/stores/app.store";
import { renderWithProviders } from "./renderWithProviders";

const { searchFilesMock, getFilesPageMock } = vi.hoisted(() => ({
  searchFilesMock: vi.fn(),
  getFilesPageMock: vi.fn(),
}));

vi.mock("../src/services/files.service", () => ({
  SEARCH_PAGE_SIZE: 100,
  filesService: {
    getFilesPage: getFilesPageMock,
    searchFiles: searchFilesMock,
    getOverview: vi.fn(async () => ({
      file_count: 0,
      directory_count: 0,
      total_bytes: 0,
      recent_files: [],
    })),
    getFavorites: vi.fn(async () => []),
  },
}));

describe("app bar global search", () => {
  beforeEach(() => {
    setActivePinia(createPinia());
    vi.stubGlobal("matchMedia", (query: string) => ({
      matches: false,
      media: query,
      onchange: null,
      addListener: () => {},
      removeListener: () => {},
      addEventListener: () => {},
      removeEventListener: () => {},
      dispatchEvent: () => false,
    }));
    getFilesPageMock.mockReset();
    getFilesPageMock.mockResolvedValue({
      items: [],
      total: 0,
      limit: 200,
      offset: 0,
      has_more: false,
    });
    const backing = new Map<string, string>();
    Object.defineProperty(globalThis, "localStorage", {
      value: {
        getItem: (key: string) => backing.get(key) ?? null,
        setItem: (key: string, value: string) =>
          backing.set(key, String(value)),
        removeItem: (key: string) => backing.delete(key),
        clear: () => backing.clear(),
        key: (index: number) => Array.from(backing.keys())[index] ?? null,
        get length() {
          return backing.size;
        },
      },
      configurable: true,
      writable: true,
    });
    searchFilesMock.mockReset();
    searchFilesMock.mockResolvedValue({
      items: [],
      hasMore: false,
      limit: 100,
      offset: 0,
    });
  });

  it("keeps a pending query in the store until the browser consumes it", () => {
    const store = useAppStore();

    store.requestSearch("  report  ");
    expect(store.pendingSearch).toBe("report");

    store.clearPendingSearch();
    expect(store.pendingSearch).toBeNull();

    // 空白查询不入队
    store.requestSearch("   ");
    expect(store.pendingSearch).toBeNull();
  });

  it("runs a search when the app bar requests one", async () => {
    // renderWithProviders 会安装自己的 pinia，这里取它激活的那个 store
    renderWithProviders(FileBrowser as any);
    const store = useAppStore();

    store.requestSearch("needle");

    await waitFor(() => {
      // 第二个参数是搜索模式（文件名 / 文件名+内容）
      expect(searchFilesMock).toHaveBeenCalledWith(
        "needle",
        expect.anything(),
        expect.objectContaining({ offset: 0 }),
      );
    });
    // 消费后清空，避免重复执行
    expect(store.pendingSearch).toBeNull();
  });
});
