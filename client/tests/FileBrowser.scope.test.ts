import { render, screen, waitFor } from "@testing-library/vue";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { createPinia, setActivePinia } from "pinia";
import { nextTick } from "vue";
import FileBrowser from "../src/components/file-browser/FileBrowser.vue";
import { useAuthStore } from "../src/stores/auth.store";
import { useFilesStore } from "../src/stores/files.store";

const { getFilesPageMock, getDirectoriesPageMock } = vi.hoisted(() => ({
  getFilesPageMock: vi.fn(),
  getDirectoriesPageMock: vi.fn(),
}));

vi.mock("../src/services/files.service", () => ({
  SEARCH_PAGE_SIZE: 100,
  MAX_MOVE_BATCH_ITEMS: 500,
  MAX_TRANSFER_PATHS: 500,
  filesService: {
    getFilesPage: getFilesPageMock,
    getDirectoriesPage: getDirectoriesPageMock,
    getOverview: vi.fn(async () => ({
      file_count: 0,
      directory_count: 0,
      total_bytes: 0,
      categories: [],
      recent_files: [],
    })),
    getFavorites: vi.fn(async () => ({
      items: [],
      total: 0,
      limit: 50,
      offset: 0,
      has_more: false,
    })),
    searchFiles: vi.fn(async () => ({
      items: [],
      hasMore: false,
      limit: 100,
      offset: 0,
    })),
  },
}));

function installLocalStorageStub() {
  const backing = new Map<string, string>();
  Object.defineProperty(globalThis, "localStorage", {
    value: {
      getItem: (key: string) => (backing.has(key) ? backing.get(key)! : null),
      setItem: (key: string, value: string) => backing.set(key, String(value)),
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
}

function stubWideScreen() {
  vi.stubGlobal("matchMedia", (query: string) => ({
    matches: query.includes("min-width: 1280px"),
    media: query,
    onchange: null,
    addListener: () => {},
    removeListener: () => {},
    addEventListener: () => {},
    removeEventListener: () => {},
    dispatchEvent: () => false,
  }));
}

function renderWithScope(scope: {
  restricted: boolean;
  label: string | null;
  roots: string[];
  canWrite: boolean;
}, currentPath = "") {
  const pinia = createPinia();
  setActivePinia(pinia);
  const auth = useAuthStore(pinia);
  auth.$patch({ scope });
  const files = useFilesStore(pinia);
  files.currentPath = currentPath;
  return {
    ...render(FileBrowser as any, { global: { plugins: [pinia] } }),
    files,
  };
}

describe("FileBrowser.vue restricted scope", () => {
  beforeEach(() => {
    installLocalStorageStub();
    localStorage.setItem(
      "vfiles:file-browser:view",
      JSON.stringify({
        mode: "list",
        sortField: "name",
        sortDirection: "asc",
        foldersFirst: true,
        thumbnailSize: 144,
        detailsVisible: false,
        // 受限范围根不是命名空间根，这里仍显式关闭卡片首页，保持列表断言语义。
        libraryCards: false,
      }),
    );
    stubWideScreen();
    getFilesPageMock.mockReset();
    getFilesPageMock.mockResolvedValue({
      items: [],
      total: 0,
      limit: 200,
      offset: 0,
      has_more: false,
    });
    getDirectoriesPageMock.mockReset();
    getDirectoriesPageMock.mockResolvedValue({
      items: [],
      total: 0,
      limit: 200,
      offset: 0,
      has_more: false,
    });
  });

  it("redirects an out-of-scope path to the first scope root", async () => {
    renderWithScope(
      {
        restricted: true,
        label: "read-docs",
        roots: ["docs"],
        canWrite: false,
      },
      "secret",
    );

    await waitFor(() =>
      expect(getFilesPageMock).toHaveBeenCalledWith(
        "docs",
        expect.anything(),
      ),
    );
    expect(screen.getByText(/受限访问：docs（只读）/)).toBeInTheDocument();
  });

  it("hides navigation above the root and read-only write actions", async () => {
    const { container } = renderWithScope(
      {
        restricted: true,
        label: "read-docs",
        roots: ["docs"],
        canWrite: false,
      },
      "docs",
    );

    await waitFor(() =>
      expect(screen.getByText(/受限访问：docs（只读）/)).toBeInTheDocument(),
    );
    const breadcrumbBar = container.querySelector(".breadcrumb-bar");
    expect(breadcrumbBar?.textContent).not.toContain("根目录");
    expect(breadcrumbBar?.textContent).toContain("docs");
    // 目录树/概览已迁到页面级 ViewsSidebar，FileBrowser 不再自带侧栏。
    expect(container.querySelector(".directory-tree")).toBeNull();
    expect(container.querySelector(".sidebar-overview")).toBeNull();
    expect(container.querySelector(".desktop-upload-split")).toBeNull();
    expect(container.querySelector(".desktop-create-folder")).toBeNull();
    expect(
      container.querySelector('.desktop-command-group button[title="批量选择"]'),
    ).toBeNull();
  });

  it("keeps write actions for a read-write scoped credential", async () => {
    const { container } = renderWithScope(
      {
        restricted: true,
        label: "write-docs",
        roots: ["docs"],
        canWrite: true,
      },
      "docs",
    );

    await waitFor(() =>
      expect(screen.getByText(/受限访问：docs（可写）/)).toBeInTheDocument(),
    );
    expect(container.querySelector(".desktop-upload-split")).not.toBeNull();
    expect(container.querySelector(".desktop-create-folder")).not.toBeNull();
  });

  it("shows a 403 message and stops loading without retrying forever", async () => {
    // 移动端布局不渲染目录树，确保只统计文件列表这一次请求。
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
    getFilesPageMock.mockRejectedValue({ status: 403 });

    const { files } = renderWithScope(
      {
        restricted: true,
        label: "read-docs",
        roots: ["docs"],
        canWrite: false,
      },
      "docs",
    );

    await waitFor(() =>
      expect(screen.getAllByText("无权访问该目录").length).toBeGreaterThan(0),
    );
    await nextTick();
    expect(files.loading).toBe(false);
    expect(getFilesPageMock).toHaveBeenCalledTimes(1);
  });

  it("does not show the scope badge for a full-scope session", async () => {
    const { container } = renderWithScope({
      restricted: false,
      label: null,
      roots: [],
      canWrite: true,
    });

    await waitFor(() => expect(getFilesPageMock).toHaveBeenCalled());
    expect(screen.queryByText(/受限访问/)).toBeNull();
    // 全量会话正常渲染文件区；旧的目录树入口不再出现在 FileBrowser 内。
    expect(container.querySelector(".file-browser")).not.toBeNull();
    expect(container.querySelector(".directory-tree")).toBeNull();
  });
});
