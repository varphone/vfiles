import { fireEvent, screen, waitFor, within } from "@testing-library/vue";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { nextTick } from "vue";
import { renderWithProviders } from "./renderWithProviders";
import { useAuthStore } from "../src/stores/auth.store";
import FileBrowser from "../src/components/file-browser/FileBrowser.vue";
import { confirmDialog } from "../src/composables/dialog";

type PageOpts = { commit?: string; limit?: number; offset?: number };
type PageResult = {
  items: unknown[];
  total: number;
  limit: number;
  offset: number;
  has_more: boolean;
};

const {
  getFilesMock,
  getFilesPageMock,
  searchFilesMock,
  deleteFileMock,
  getFileContentMock,
  movePathMock,
  listTransferTargetsMock,
  transferOwnershipMock,
} = vi.hoisted(() => ({
  getFilesMock: vi.fn(
    async (_path: string, _commit?: string): Promise<unknown[]> => [],
  ),
  getFilesPageMock:
    vi.fn<(path: string, opts?: PageOpts) => Promise<PageResult>>(),
  searchFilesMock: vi.fn(async (): Promise<unknown[]> => []),
  deleteFileMock: vi.fn(async () => ({ success: true })),
  getFileContentMock: vi.fn(async () => new Blob(["hello preview"])),
  movePathMock: vi.fn(async () => ({ success: true })),
  listTransferTargetsMock: vi.fn(async (): Promise<unknown[]> => []),
  transferOwnershipMock: vi.fn(async () => ({
    transferred: 1,
    target_username: "alice",
  })),
}));

vi.mock("../src/composables/dialog", () => ({
  confirmDialog: vi.fn(async () => true),
  promptDialog: vi.fn(async () => null),
}));

// 服务端分页：以 getFilesMock 为全量数据源切片，保持既有断言不变。
getFilesPageMock.mockImplementation(
  async (
    path: string,
    opts?: { commit?: string; limit?: number; offset?: number },
  ) => {
    const all = (await getFilesMock(path, opts?.commit)) as unknown[];
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
  SEARCH_PAGE_SIZE: 100,
  filesService: {
    getFiles: getFilesMock,
    getFilesPage: getFilesPageMock,
    listTransferTargets: listTransferTargetsMock,
    transferOwnership: transferOwnershipMock,
    // 侧栏概览与收藏：不 stub 时会在挂载后产生未处理的 rejection
    getOverview: vi.fn(async () => ({
      file_count: 0,
      directory_count: 0,
      total_size_bytes: 0,
      recent_files: [],
    })),
    getFavorites: vi.fn(async () => []),
    // 组合式函数读取分页信封，这里把「数据源 mock」包一层
    searchFiles: vi.fn(async (...args: unknown[]) => {
      const result = await (
        searchFilesMock as (...a: unknown[]) => Promise<unknown>
      )(...args);
      if (Array.isArray(result)) {
        return { items: result, hasMore: false, limit: 100, offset: 0 };
      }
      return result;
    }),
    deleteFile: deleteFileMock,
    getFileContent: getFileContentMock,
    movePath: movePathMock,
  },
}));

/**
 * 视图偏好存进 localStorage，测试环境（bun + jsdom）的实现不完整，
 * 这里提供确定性实现，便于显式控制「详细信息」面板的开关。
 */
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

/** 桌面端默认会渲染右侧详细信息面板；列表相关断言需要关掉它避免文本重复。 */
function setDetailsVisible(visible: boolean) {
  localStorage.setItem(
    "vfiles:file-browser:view",
    JSON.stringify({
      mode: "list",
      sortField: "name",
      sortDirection: "asc",
      foldersFirst: true,
      thumbnailSize: 144,
      detailsVisible: visible,
    }),
  );
}

function stubBrowserApis() {
  vi.stubGlobal("matchMedia", (q: string) => ({
    matches: false,
    media: q,
    onchange: null,
    addListener: () => {},
    removeListener: () => {},
    addEventListener: () => {},
    removeEventListener: () => {},
    dispatchEvent: () => false,
  }));
  vi.stubGlobal(
    "confirm",
    vi.fn(() => true),
  );
}

// 所有 describe 共用：重置服务 mock 并补齐浏览器 API（matchMedia 等）。
beforeEach(() => {
  installLocalStorageStub();
  setDetailsVisible(false);
  getFilesMock.mockReset();
  getFilesPageMock.mockClear();
  searchFilesMock.mockReset();
  deleteFileMock.mockReset();
  getFileContentMock.mockReset();
  movePathMock.mockReset();
  movePathMock.mockResolvedValue({ success: true });
  getFilesMock.mockResolvedValue([]);
  searchFilesMock.mockResolvedValue([]);
  deleteFileMock.mockResolvedValue({ success: true });
  getFileContentMock.mockResolvedValue(new Blob(["hello preview"]));
  stubBrowserApis();
});

describe("FileBrowser.vue", () => {
  it("shows empty folder message when no files", async () => {
    const { findByText } = renderWithProviders(FileBrowser as any);
    await findByText("此文件夹为空");
  });

  it("refreshes search results after deleting a searched file", async () => {
    vi.stubGlobal("matchMedia", (q: string) => ({
      matches: true,
      media: q,
      onchange: null,
      addListener: () => {},
      removeListener: () => {},
      addEventListener: () => {},
      removeEventListener: () => {},
      dispatchEvent: () => false,
    }));

    searchFilesMock
      .mockResolvedValueOnce([
        {
          id: "file-1",
          name: "match.txt",
          path: "docs/match.txt",
          kind: "file",
          size_bytes: 12,
          created_at: "2026-04-09T00:00:00.000Z",
        },
      ])
      .mockResolvedValueOnce([]);

    const { findAllByText, findByPlaceholderText, findByText, queryByText } =
      renderWithProviders(FileBrowser as any);

    const input = await findByPlaceholderText("搜索文件名...");
    await fireEvent.update(input, "match");
    await fireEvent.keyUp(input, { key: "Enter", code: "Enter", charCode: 13 });

    await findByText("找到 1 项");
    const [fileName] = await findAllByText(
      (_, element) => element?.textContent === "match.txt",
    );
    await fireEvent.click(fileName.closest(".file-item") || fileName);
    await fireEvent.click(await findByText("删除"));

    await waitFor(() => {
      expect(deleteFileMock).toHaveBeenCalledWith(
        "docs/match.txt",
        expect.stringContaining("删除文件"),
      );
      expect(searchFilesMock).toHaveBeenCalledTimes(2);
    });

    await findByText("没有找到匹配的文件");
    expect(queryByText("找到 1 项")).not.toBeInTheDocument();
  });

  it("renders grid cards and applies the selected sort order", async () => {
    getFilesMock.mockResolvedValue([
      {
        id: "b",
        name: "b.txt",
        path: "b.txt",
        kind: "file",
        size_bytes: 2048,
        created_at: "2026-04-09T00:00:00.000Z",
        updated_at: "2026-04-09T00:00:00.000Z",
      },
      {
        id: "a",
        name: "a.txt",
        path: "a.txt",
        kind: "file",
        size_bytes: 10,
        created_at: "2026-04-10T00:00:00.000Z",
        updated_at: "2026-04-10T00:00:00.000Z",
      },
    ]);

    const { container, findByText } = renderWithProviders(FileBrowser as any);
    await findByText("b.txt");

    const { useFileViewStore } = await import("../src/stores/fileView.store");
    const view = useFileViewStore();
    view.setMode("grid");
    expect(view.mode).toBe("grid");

    await waitFor(() => {
      const names = Array.from(
        container.querySelectorAll(".file-card-name"),
      ).map((el) => el.textContent?.trim());
      expect(names).toEqual(["a.txt", "b.txt"]);
      expect(container.querySelectorAll(".file-card").length).toBe(2);
    });
  });

  it("selects every entry with Ctrl+A and exits batch mode with Escape", async () => {
    getFilesMock.mockResolvedValue([
      {
        id: "a",
        name: "a.txt",
        path: "a.txt",
        kind: "file",
        size_bytes: 10,
        created_at: "2026-04-10T00:00:00.000Z",
        updated_at: "2026-04-10T00:00:00.000Z",
      },
      {
        id: "b",
        name: "b.txt",
        path: "b.txt",
        kind: "file",
        size_bytes: 20,
        created_at: "2026-04-11T00:00:00.000Z",
        updated_at: "2026-04-11T00:00:00.000Z",
      },
    ]);

    const { findAllByText, findByText, queryByText } = renderWithProviders(
      FileBrowser as any,
    );
    await findByText("a.txt");

    await fireEvent.keyDown(document, { key: "a", ctrlKey: true });
    expect((await findAllByText("已选 2 项")).length).toBeGreaterThan(0);

    await fireEvent.keyDown(document, { key: "Escape" });
    await waitFor(() => {
      expect(queryByText("已选 2 项")).not.toBeInTheDocument();
    });
  });

  it("ignores shortcuts while typing in an input", async () => {
    getFilesMock.mockResolvedValue([
      {
        id: "a",
        name: "a.txt",
        path: "a.txt",
        kind: "file",
        size_bytes: 10,
        created_at: "2026-04-10T00:00:00.000Z",
        updated_at: "2026-04-10T00:00:00.000Z",
      },
    ]);

    const { findByText, findByPlaceholderText, queryByText } =
      renderWithProviders(FileBrowser as any);
    await findByText("a.txt");

    const input = await findByPlaceholderText("搜索名称、扩展名或路径");
    await fireEvent.keyDown(input, { key: "a", ctrlKey: true });

    expect(queryByText("已选 1 项")).not.toBeInTheDocument();
  });

  it("supports ctrl-click plus shift-click range selection", async () => {
    getFilesMock.mockResolvedValue([
      {
        id: "a",
        name: "a.txt",
        path: "a.txt",
        kind: "file",
        size_bytes: 1,
        created_at: "2026-04-10T00:00:00.000Z",
        updated_at: "2026-04-10T00:00:00.000Z",
      },
      {
        id: "b",
        name: "b.txt",
        path: "b.txt",
        kind: "file",
        size_bytes: 2,
        created_at: "2026-04-11T00:00:00.000Z",
        updated_at: "2026-04-11T00:00:00.000Z",
      },
      {
        id: "c",
        name: "c.txt",
        path: "c.txt",
        kind: "file",
        size_bytes: 3,
        created_at: "2026-04-12T00:00:00.000Z",
        updated_at: "2026-04-12T00:00:00.000Z",
      },
    ]);

    const { findAllByText, findByText } = renderWithProviders(
      FileBrowser as any,
    );
    await findByText("a.txt");

    const rowOf = async (name: string) =>
      (await findAllByText(name))[0].closest("tr")!;

    await fireEvent.click(await rowOf("a.txt"), { ctrlKey: true });
    await fireEvent.click(await rowOf("c.txt"), { shiftKey: true });

    expect((await findAllByText("已选 3 项")).length).toBeGreaterThan(0);
  });

  it("opens a context menu and runs the chosen action", async () => {
    getFilesMock.mockResolvedValue([
      {
        id: "a",
        name: "a.txt",
        path: "a.txt",
        kind: "file",
        size_bytes: 1,
        created_at: "2026-04-10T00:00:00.000Z",
        updated_at: "2026-04-10T00:00:00.000Z",
      },
    ]);

    const { findByText, findByRole } = renderWithProviders(FileBrowser as any);
    const name = await findByText("a.txt");

    await fireEvent.contextMenu(name.closest("tr")!, {
      clientX: 40,
      clientY: 30,
    });

    const deleteItem = await findByRole("menuitem", { name: "删除" });
    await fireEvent.click(deleteItem);

    await waitFor(() => {
      expect(deleteFileMock).toHaveBeenCalledWith(
        "a.txt",
        expect.stringContaining("删除文件"),
      );
    });
  });

  it("closes the context menu on an outside click and can reopen it", async () => {
    getFilesMock.mockResolvedValue([
      {
        id: "a",
        name: "a.txt",
        path: "a.txt",
        kind: "file",
        size_bytes: 1,
        created_at: "2026-04-10T00:00:00.000Z",
        updated_at: "2026-04-10T00:00:00.000Z",
      },
    ]);

    const { findByText, findByRole, queryByRole } = renderWithProviders(
      FileBrowser as any,
    );
    const name = await findByText("a.txt");
    const row = name.closest("tr")!;

    await fireEvent.contextMenu(row, { clientX: 10, clientY: 10 });
    expect(
      await findByRole("menuitem", { name: "重命名" }),
    ).toBeInTheDocument();

    await fireEvent.click(document.body);
    await waitFor(() => {
      expect(queryByRole("menuitem", { name: "重命名" })).toBeNull();
    });

    // 再次打开仍然可用（监听器不应在关闭时被移除）
    await fireEvent.contextMenu(row, { clientX: 12, clientY: 12 });
    expect(
      await findByRole("menuitem", { name: "重命名" }),
    ).toBeInTheDocument();

    await fireEvent.click(document.body);
    await waitFor(() => {
      expect(queryByRole("menuitem", { name: "重命名" })).toBeNull();
    });
  });

  it("ignores a stale search response that resolves after a newer one", async () => {
    let resolveSlow!: (value: unknown[]) => void;
    const slowSearch = new Promise<unknown[]>((resolve) => {
      resolveSlow = resolve;
    });

    searchFilesMock.mockReturnValueOnce(slowSearch);
    searchFilesMock.mockResolvedValueOnce([
      {
        id: "fast",
        name: "fast.txt",
        path: "fast.txt",
        kind: "file",
        created_at: "2026-04-10T00:00:00.000Z",
        updated_at: "2026-04-10T00:00:00.000Z",
      },
    ]);

    const { findByPlaceholderText } = renderWithProviders(FileBrowser as any);

    const input = await findByPlaceholderText("搜索名称、扩展名或路径");
    await fireEvent.update(input, "slow");
    await fireEvent.keyUp(input, { key: "Enter", code: "Enter", charCode: 13 });
    await fireEvent.update(input, "fast");
    await fireEvent.keyUp(input, { key: "Enter", code: "Enter", charCode: 13 });

    // 命中片段会被 <mark> 拆成多个节点，这里用整体文本判断
    await waitFor(() => {
      expect(document.body.textContent).toContain("fast.txt");
    });

    resolveSlow([
      {
        id: "slow",
        name: "slow.txt",
        path: "slow.txt",
        kind: "file",
        created_at: "2026-04-10T00:00:00.000Z",
        updated_at: "2026-04-10T00:00:00.000Z",
      },
    ]);

    await waitFor(() => {
      expect(document.body.textContent).not.toContain("slow.txt");
    });
    expect(document.body.textContent).toContain("fast.txt");
  });

  it("offers a retry button when loading fails", async () => {
    getFilesMock.mockRejectedValueOnce(new Error("网络错误"));
    getFilesMock.mockResolvedValueOnce([
      {
        id: "a",
        name: "a.txt",
        path: "a.txt",
        kind: "file",
        size_bytes: 1,
        created_at: "2026-04-10T00:00:00.000Z",
        updated_at: "2026-04-10T00:00:00.000Z",
      },
    ]);

    const { findByRole, findByText } = renderWithProviders(FileBrowser as any);

    await findByText("网络错误");
    expect(getFilesMock).toHaveBeenCalledTimes(1);

    await fireEvent.click(await findByRole("button", { name: /重试/ }));

    await findByText("a.txt");
    expect(getFilesMock).toHaveBeenCalledTimes(2);
  });
});

describe("FileBrowser.vue preview navigation", () => {
  it("opens a preview and moves to the next file with the arrow key", async () => {
    getFilesMock.mockResolvedValue([
      {
        id: "a",
        name: "a.txt",
        path: "a.txt",
        kind: "file",
        size_bytes: 1,
        created_at: "2026-04-10T00:00:00.000Z",
        updated_at: "2026-04-10T00:00:00.000Z",
      },
      {
        id: "b",
        name: "b.txt",
        path: "b.txt",
        kind: "file",
        size_bytes: 2,
        created_at: "2026-04-11T00:00:00.000Z",
        updated_at: "2026-04-11T00:00:00.000Z",
      },
    ]);

    const { findByText, findAllByText } = renderWithProviders(
      FileBrowser as any,
    );
    const names = await findAllByText("a.txt");
    await fireEvent.dblClick(names[0].closest("tr")!);

    // 灯箱里文件名的位置由顶栏承担（模态标题只写「预览」）
    const previewName = async (name: string) => {
      await waitFor(() =>
        expect(
          document.querySelector(".preview-toolbar-name")?.textContent?.trim(),
        ).toBe(name),
      );
    };

    await findByText("预览");
    await previewName("a.txt");

    await fireEvent.keyDown(document, { key: "ArrowRight" });
    await previewName("b.txt");

    await fireEvent.keyDown(document, { key: "ArrowLeft" });
    await previewName("a.txt");
  });
});

describe("FileBrowser.vue large directories", () => {
  it("renders the first page only and offers to load more", async () => {
    getFilesMock.mockResolvedValue(
      Array.from({ length: 45 }, (_, index) => ({
        id: `f${index}`,
        name: `f${index}.txt`,
        path: `f${index}.txt`,
        kind: "file",
        size_bytes: index + 1,
        created_at: "2026-04-10T00:00:00.000Z",
        updated_at: "2026-04-10T00:00:00.000Z",
      })),
    );

    const { findByText, container } = renderWithProviders(FileBrowser as any);
    await findByText("f0.txt");

    // 首批 40 条（另有 `.`/`..` 两个快捷项共 47 条）
    expect(container.querySelectorAll("tbody tr")).toHaveLength(40);
    expect(container.textContent).toContain("继续下滑加载更多");
    expect(container.textContent).toContain("已显示 40 / 47");
    expect(container.textContent).not.toContain("f44.txt");
  });

  it("shows page progress when the directory has more server pages", async () => {
    getFilesMock.mockResolvedValue(
      Array.from({ length: 250 }, (_, index) => ({
        id: `f${index}`,
        name: `f${index}.txt`,
        path: `f${index}.txt`,
        kind: "file",
        size_bytes: index + 1,
        created_at: "2026-04-10T00:00:00.000Z",
        updated_at: "2026-04-10T00:00:00.000Z",
      })),
    );

    const { findByText, container } = renderWithProviders(FileBrowser as any);
    await findByText("f0.txt");

    // 服务端返回第一页 200 条，界面提示“已加载 / 总数”，并说明还有更多。
    expect(container.textContent).toContain("当前目录 200 / 250 项");
    expect(container.textContent).toContain("已显示 40 / 252");
    expect(container.textContent).toContain("继续下滑加载更多");
    expect(getFilesPageMock).toHaveBeenCalledWith(
      "",
      expect.objectContaining({ offset: 0, limit: 200 }),
    );
  });
});
describe("FileBrowser.vue drag and drop", () => {
  it("moves a dragged file into a dropped-on folder", async () => {
    getFilesMock.mockResolvedValue([
      {
        id: "d",
        name: "docs",
        path: "docs",
        kind: "directory",
        created_at: "2026-04-10T00:00:00.000Z",
        updated_at: "2026-04-10T00:00:00.000Z",
      },
      {
        id: "a",
        name: "a.txt",
        path: "a.txt",
        kind: "file",
        size_bytes: 1,
        created_at: "2026-04-10T00:00:00.000Z",
        updated_at: "2026-04-10T00:00:00.000Z",
      },
    ]);

    const { findAllByText, findByText } = renderWithProviders(
      FileBrowser as any,
    );
    await findByText("docs");

    const fileRow = (await findAllByText("a.txt"))[0].closest("tr")!;
    const dirRow = (await findAllByText("docs"))[0].closest("tr")!;

    await fireEvent.dragStart(fileRow);
    await fireEvent.drop(dirRow);

    await waitFor(() => {
      expect(movePathMock).toHaveBeenCalledWith(
        "a.txt",
        "docs/a.txt",
        expect.stringContaining("移动文件"),
      );
    });
  });
});
describe("FileBrowser.vue loading state", () => {
  it("shows a skeleton while the directory loads", async () => {
    let resolveFiles!: (value: unknown[]) => void;
    getFilesMock.mockReturnValueOnce(
      new Promise<unknown[]>((resolve) => {
        resolveFiles = resolve;
      }),
    );

    const { findByRole, findByText } = renderWithProviders(FileBrowser as any);

    const status = await findByRole("status");
    expect(status).toHaveAttribute("aria-busy", "true");

    resolveFiles([]);
    await findByText("此文件夹为空");
  });
});
describe("FileBrowser.vue create folder entry", () => {
  it("no longer renders navigation shortcut rows", async () => {
    setDetailsVisible(false);
    getFilesMock.mockResolvedValue([
      {
        id: "docs",
        name: "docs",
        path: "docs",
        kind: "directory",
        created_at: "2026-04-10T00:00:00.000Z",
        updated_at: "2026-04-10T00:00:00.000Z",
      },
    ]);

    const { findByText, container } = renderWithProviders(FileBrowser as any);
    await findByText("docs");

    const names = Array.from(
      container.querySelectorAll("tr.desktop-file-row .desktop-name-text"),
    ).map((el) => el.textContent?.trim());
    expect(names).toEqual(["docs"]);
    expect(names).not.toContain(".");
    expect(names).not.toContain("..");
  });

  it("asks for a folder name from the toolbar button", async () => {
    setDetailsVisible(false);
    getFilesMock.mockResolvedValue([]);
    const { promptDialog } = await import("../src/composables/dialog");

    const { findByLabelText } = renderWithProviders(FileBrowser as any);

    await fireEvent.click(await findByLabelText("新建文件夹"));

    // 复用既有的「新建目录」提示流程（对话框由 DialogHost 渲染）
    await waitFor(() =>
      expect(promptDialog).toHaveBeenCalledWith(
        expect.objectContaining({ title: "新建目录" }),
      ),
    );
  });
});

describe("FileBrowser.vue action column", () => {
  function files() {
    return [
      {
        id: "a",
        name: "a.txt",
        path: "a.txt",
        kind: "file",
        size_bytes: 10,
        created_at: "2026-04-10T00:00:00.000Z",
        updated_at: "2026-04-10T00:00:00.000Z",
      },
    ];
  }

  it("hides the action column while the details panel is visible", async () => {
    // 桌面端默认显示详情面板（localStorage 里 detailsVisible: true）
    setDetailsVisible(true);
    getFilesMock.mockResolvedValue(files());

    const { container } = renderWithProviders(FileBrowser as any);
    // 详情面板里也会出现文件名，这里等列表行渲染出来即可
    await waitFor(() =>
      expect(container.querySelector("tr.desktop-file-row")).not.toBeNull(),
    );

    expect(container.querySelector(".file-list-actions-header")).toBeNull();
    // 表格列数：名称/修改时间/类型/大小（不含操作）
    const row = container.querySelector("tr.desktop-file-row")!;
    expect(row.querySelectorAll("td").length).toBe(5);
  });

  it("keeps a per-row menu available while the action column is hidden", async () => {
    // 这就是用户反馈的场景：详情面板开着、操作列隐藏，仍要能直接操作某一行
    setDetailsVisible(true);
    getFilesMock.mockResolvedValue(files());

    const { container } = renderWithProviders(FileBrowser as any);
    await waitFor(() =>
      expect(container.querySelector("tr.desktop-file-row")).not.toBeNull(),
    );

    const row = container.querySelector("tr.desktop-file-row")!;
    const menuButton = row.querySelector(".desktop-row-menu");
    expect(menuButton).not.toBeNull();
    expect(menuButton!.getAttribute("aria-label")).toContain("a.txt");

    await fireEvent.click(menuButton!);
    await waitFor(() =>
      expect(document.querySelector(".vfiles-context-menu")).not.toBeNull(),
    );
    // 菜单里应包含下载/重命名/移动等常用操作
    const labels = Array.from(
      document.querySelectorAll(".vfiles-context-menu [role='menuitem']"),
    ).map((item) =>
      item.querySelector(".vfiles-context-menu__label")?.textContent?.trim(),
    );
    expect(labels).toEqual(
      expect.arrayContaining(["下载", "重命名", "移动", "删除"]),
    );
    // 加速键标注（r121-122 ✓ Finder/Explorer 菜单标配：重命名=F2、删除=Del）
    const accels = Array.from(
      document.querySelectorAll(".vfiles-context-menu__accel"),
    ).map((el) => el.textContent?.trim());
    expect(accels).toEqual(expect.arrayContaining(["F2", "Del"]));
  });

  it("reveals the batch action bar as soon as a row checkbox is ticked", async () => {
    setDetailsVisible(true);
    getFilesMock.mockResolvedValue(files());

    const { container } = renderWithProviders(FileBrowser as any);
    // 详情面板里也有文件名的文本，这里只等列表行渲染
    await waitFor(() =>
      expect(container.querySelector("tr.desktop-file-row")).not.toBeNull(),
    );

    // 未进入批量选择模式时，行内也应提供复选框（hover 出现）
    const row = container.querySelector("tr.desktop-file-row")!;
    const checkbox = row.querySelector(
      'input[type="checkbox"]',
    ) as HTMLInputElement;
    expect(checkbox).not.toBeNull();

    await fireEvent.change(checkbox);

    // 勾选后无需再点工具栏「批量选择」，操作条直接出现
    await waitFor(() =>
      expect(container.querySelector(".desktop-batch-strip")).not.toBeNull(),
    );
    const bar = container.querySelector(".desktop-batch-strip");
    expect(bar).not.toBeNull();
    expect(bar!.textContent).toContain("下载");
    expect(bar!.textContent).toContain("移动");
    expect(bar!.textContent).toContain("重命名");
  });

  it("keeps the selection bar inside the list column so it can stick", async () => {
    setDetailsVisible(true);
    getFilesMock.mockResolvedValue(files());

    const { container } = renderWithProviders(FileBrowser as any);
    await waitFor(() =>
      expect(container.querySelector("tr.desktop-file-row")).not.toBeNull(),
    );

    const row = container.querySelector("tr.desktop-file-row")!;
    await fireEvent.change(row.querySelector('input[type="checkbox"]')!);

    await waitFor(() =>
      expect(container.querySelector(".desktop-batch-strip")).not.toBeNull(),
    );
    const bar = container.querySelector(".desktop-batch-strip")!;
    // 放在列表列内部：吸顶范围覆盖整个列表，且不会横跨详情面板
    expect(bar.closest(".desktop-list-primary-shell")).not.toBeNull();
    expect(bar.closest(".file-browser-toolbar")).toBeNull();
  });

  it("opens the row menu from the keyboard", async () => {
    setDetailsVisible(true);
    getFilesMock.mockResolvedValue(files());

    const { container } = renderWithProviders(FileBrowser as any);
    await waitFor(() =>
      expect(container.querySelector("tr.desktop-file-row")).not.toBeNull(),
    );

    // 先用方向键把活动行定位到列表，再按 Shift+F10
    await fireEvent.keyDown(document, { key: "ArrowDown" });
    await fireEvent.keyDown(document, { key: "F10", shiftKey: true });

    await waitFor(() =>
      expect(document.querySelector(".vfiles-context-menu")).not.toBeNull(),
    );
  });

  it("shows the action column when the details panel is hidden", async () => {
    setDetailsVisible(false);
    getFilesMock.mockResolvedValue(files());

    const { findByText, container } = renderWithProviders(FileBrowser as any);
    await findByText("a.txt");

    expect(container.querySelector(".file-list-actions-header")).not.toBeNull();
    const row = container.querySelector("tr.desktop-file-row")!;
    expect(row.querySelectorAll("td").length).toBe(6);
  });
});

describe("FileBrowser.vue empty and error states", () => {
  it("shows a designed empty state with upload and new-folder actions", async () => {
    setDetailsVisible(false);
    getFilesMock.mockResolvedValue([]);

    const { findByText, container } = renderWithProviders(FileBrowser as any);
    await findByText("此文件夹为空");

    // 列表区域可能随后再渲染一次，统一用 waitFor 重新查询当前 DOM
    await waitFor(() =>
      expect(
        container.querySelector(".empty-state .empty-state-illustration svg"),
      ).not.toBeNull(),
    );
    await waitFor(() => {
      const labels = Array.from(
        container.querySelectorAll(".empty-state-actions button"),
      ).map((button) => button.textContent?.trim());
      expect(labels).toEqual(["上传文件", "新建文件夹"]);
    });
  });

  it("wraps a load failure in the error state with retry", async () => {
    setDetailsVisible(false);
    getFilesMock.mockRejectedValue(new Error("网络不可用"));

    const { findByText, container } = renderWithProviders(FileBrowser as any);
    await findByText("加载失败");

    await waitFor(() =>
      expect(container.querySelector(".empty-state.is-error")).not.toBeNull(),
    );
    const state = container.querySelector(".empty-state.is-error")!;
    expect(state.textContent).toContain("网络不可用");
    expect(state.textContent).toContain("重试");
  });

  it("offers switching to name search when content search finds nothing", async () => {
    setDetailsVisible(false);
    getFilesMock.mockResolvedValue([]);
    searchFilesMock.mockResolvedValue([]);

    const { findByText, container } = renderWithProviders(FileBrowser as any);
    await findByText("此文件夹为空");

    // 该用例需要服务端开启内容搜索能力
    const auth = useAuthStore();
    auth.features = {
      authEnabled: true,
      multiUser: false,
      emailLogin: false,
      searchContent: true,
      shareEnabled: true,
      historyEnabled: true,
      ftpEnabled: false,
      maxFileSizeBytes: 4 * 1024 * 1024 * 1024,
    };
    await nextTick();

    await fireEvent.click(container.querySelector(".desktop-search-toggle")!);
    await waitFor(() =>
      expect(container.querySelector(".desktop-search-filters")).not.toBeNull(),
    );
    const contentToggle = container.querySelector<HTMLInputElement>(
      ".desktop-search-filters input[type='checkbox']",
    )!;
    expect(contentToggle.disabled).toBe(false);
    await fireEvent.click(contentToggle);

    // 勾选内容搜索后占位文案会变化，这里按 class 取输入框
    const input = container.querySelector<HTMLInputElement>(
      ".desktop-search-control",
    )!;
    await fireEvent.update(input, "不存在的关键字");
    // 搜索框用 keyup.enter 触发（keydown 不生效）
    await fireEvent.keyUp(input, { key: "Enter" });

    await findByText("没有找到匹配的文件");
    await waitFor(() =>
      expect(
        container.querySelector(".empty-state-hint")?.textContent ?? "",
      ).toContain("内容搜索"),
    );

    const switchButton = Array.from(
      container.querySelectorAll<HTMLButtonElement>(
        ".empty-state-actions button",
      ),
    ).find((button) => button.textContent?.includes("改为文件名搜索"));
    expect(switchButton).toBeTruthy();

    await fireEvent.click(switchButton!);
    // 切换后回到文件名搜索，不再显示内容搜索的替代按钮
    await waitFor(() =>
      expect(container.textContent).not.toContain("改为文件名搜索"),
    );
  });
});

describe("FileBrowser.vue view and sort menus", () => {
  it("keeps 文件夹置顶 only in the sort menu", async () => {
    setDetailsVisible(false);
    getFilesMock.mockResolvedValue([]);

    const { container, findAllByText } = renderWithProviders(
      FileBrowser as any,
    );
    await findAllByText("全部文件").catch(() => []);

    // 视图菜单：只负责显示方式与缩略图大小
    await fireEvent.click(
      container.querySelector(".view-options .vf-ghost-button")!,
    );
    await waitFor(() =>
      expect(container.querySelector(".view-options-panel")).not.toBeNull(),
    );
    const viewPanel = container.querySelector(".view-options-panel")!;
    expect(viewPanel.textContent).toContain("显示方式");
    expect(viewPanel.textContent).not.toContain("文件夹置顶");

    // 排序菜单：文件夹置顶属于排序选项
    const sortTrigger = container.querySelector(".sort-menu button")!;
    await fireEvent.click(sortTrigger);
    await waitFor(() => {
      const text = container.querySelector(".sort-menu")?.textContent ?? "";
      expect(text).toContain("文件夹置顶");
    });
  });
});

describe("FileBrowser.vue inline rename", () => {
  function files() {
    return [
      {
        id: "a",
        name: "a.txt",
        path: "a.txt",
        kind: "file",
        size_bytes: 10,
        created_at: "2026-04-10T00:00:00.000Z",
        updated_at: "2026-04-10T00:00:00.000Z",
      },
    ];
  }

  it("renames in place with Enter instead of a dialog", async () => {
    setDetailsVisible(false);
    getFilesMock.mockResolvedValue(files());
    movePathMock.mockResolvedValue({ success: true });

    const { findByLabelText, findByRole, findByText } = renderWithProviders(
      FileBrowser as any,
    );
    const name = await findByText("a.txt");

    // 右键菜单里的「重命名」现在打开内联输入框，而不是弹对话框
    await fireEvent.contextMenu(name.closest("tr")!);
    await fireEvent.click(await findByRole("menuitem", { name: "重命名" }));

    const input = await findByLabelText("重命名 a.txt");
    expect((input as HTMLInputElement).value).toBe("a.txt");

    await fireEvent.update(input, "renamed.txt");
    await fireEvent.keyDown(input, { key: "Enter" });

    await waitFor(() => {
      expect(movePathMock).toHaveBeenCalledWith(
        "a.txt",
        "renamed.txt",
        expect.stringContaining("重命名"),
      );
    });
  });

  it("cancels inline rename with Escape without calling the API", async () => {
    setDetailsVisible(false);
    getFilesMock.mockResolvedValue(files());
    movePathMock.mockClear();

    const { findByLabelText, findByRole, findByText, queryByLabelText } =
      renderWithProviders(FileBrowser as any);
    const name = await findByText("a.txt");

    await fireEvent.contextMenu(name.closest("tr")!);
    await fireEvent.click(await findByRole("menuitem", { name: "重命名" }));
    const input = await findByLabelText("重命名 a.txt");
    await fireEvent.update(input, "ignored.txt");
    await fireEvent.keyDown(input, { key: "Escape" });

    await waitFor(() => {
      expect(queryByLabelText("重命名 a.txt")).toBeNull();
    });
    expect(movePathMock).not.toHaveBeenCalled();
  });
});

describe("FileBrowser.vue keyboard navigation", () => {
  function files() {
    return ["a.txt", "b.txt", "c.txt"].map((name, index) => ({
      id: name,
      name,
      path: name,
      kind: "file",
      size_bytes: (index + 1) * 10,
      created_at: "2026-04-10T00:00:00.000Z",
      updated_at: "2026-04-10T00:00:00.000Z",
    }));
  }

  /** 单选场景下高亮行（或网格里的活动卡片）即活动项。 */
  function activeName(container: Element) {
    const row = container.querySelector(
      ".desktop-file-row.is-row-selected .desktop-name-text",
    );
    if (row) return row.textContent?.trim();
    return container
      .querySelector(".file-card--active .file-card-name")
      ?.textContent?.trim();
  }

  function selectedNames(container: Element) {
    return Array.from(
      container.querySelectorAll(".desktop-file-row.is-row-selected"),
    ).map((row) =>
      row.querySelector(".desktop-name-text")?.textContent?.trim(),
    );
  }

  it("moves the active row with the arrow keys", async () => {
    setDetailsVisible(false);
    getFilesMock.mockResolvedValue(files());

    const { findByText, container } = renderWithProviders(FileBrowser as any);
    await findByText("a.txt");

    // 初始活动行是第一个真实条目
    await waitFor(() => expect(activeName(container)).toBe("a.txt"));

    await fireEvent.keyDown(document, { key: "ArrowDown" });
    await waitFor(() => expect(activeName(container)).toBe("b.txt"));

    await fireEvent.keyDown(document, { key: "ArrowDown" });
    await waitFor(() => expect(activeName(container)).toBe("c.txt"));

    // 到底后不再移动
    await fireEvent.keyDown(document, { key: "ArrowDown" });
    await waitFor(() => expect(activeName(container)).toBe("c.txt"));

    await fireEvent.keyDown(document, { key: "ArrowUp" });
    await waitFor(() => expect(activeName(container)).toBe("b.txt"));

    await fireEvent.keyDown(document, { key: "End" });
    await waitFor(() => expect(activeName(container)).toBe("c.txt"));

    await fireEvent.keyDown(document, { key: "Home" });
    await waitFor(() => expect(activeName(container)).toBe("a.txt"));
  });

  it("extends the selection with shift and arrow keys", async () => {
    setDetailsVisible(false);
    getFilesMock.mockResolvedValue(files());

    const { findByText, container } = renderWithProviders(FileBrowser as any);
    await findByText("a.txt");
    await waitFor(() => expect(activeName(container)).toBe("a.txt"));

    // 先用 Ctrl(⌘) 点击设下区间锚点，再用 Shift+方向键扩展
    const first = await findByText("a.txt");
    await fireEvent.click(first, { ctrlKey: true });
    await fireEvent.keyDown(document, { key: "ArrowDown", shiftKey: true });

    // 区间选择会同时高亮 a.txt 与 b.txt；再按一次扩展到 c.txt
    await waitFor(() =>
      expect(selectedNames(container)).toEqual(["a.txt", "b.txt"]),
    );

    await fireEvent.keyDown(document, { key: "ArrowDown", shiftKey: true });
    await waitFor(() =>
      expect(selectedNames(container)).toEqual(["a.txt", "b.txt", "c.txt"]),
    );
  });

  it("anchors the first shift-extension at the current row", async () => {
    setDetailsVisible(false);
    getFilesMock.mockResolvedValue(files());

    const { findByText, container } = renderWithProviders(FileBrowser as any);
    await findByText("a.txt");
    await waitFor(() => expect(activeName(container)).toBe("a.txt"));

    // 没有任何点击时，第一次 Shift+↓ 应从当前行开始形成区间
    await fireEvent.keyDown(document, { key: "ArrowDown", shiftKey: true });

    await waitFor(() =>
      expect(selectedNames(container)).toEqual(["a.txt", "b.txt"]),
    );
  });

  it("moves by a row in grid view using the rendered columns", async () => {
    setDetailsVisible(false);
    getFilesMock.mockResolvedValue(
      Array.from({ length: 6 }, (_, index) => ({
        id: `f${index}`,
        name: `f${index}.txt`,
        path: `f${index}.txt`,
        kind: "file",
        size_bytes: index + 1,
        created_at: "2026-04-10T00:00:00.000Z",
        updated_at: "2026-04-10T00:00:00.000Z",
      })),
    );

    const { findByText, container } = renderWithProviders(FileBrowser as any);
    await findByText("f0.txt");
    const { useFileViewStore } = await import("../src/stores/fileView.store");
    const view = useFileViewStore();
    view.setMode("grid");
    await waitFor(() =>
      expect(container.querySelectorAll(".file-card").length).toBe(6),
    );

    // jsdom 没有布局，手动给出「每行 3 张卡」的偏移量
    const cards = Array.from(
      container.querySelectorAll<HTMLElement>(".file-card"),
    );
    cards.forEach((card, index) => {
      Object.defineProperty(card, "offsetTop", {
        value: Math.floor(index / 3) * 120,
        configurable: true,
      });
    });

    await waitFor(() => expect(activeName(container)).toBe("f0.txt"));

    // 下移一行 → 第 4 项；右移一项 → 第 5 项；上移一行 → 第 2 项
    await fireEvent.keyDown(document, { key: "ArrowDown" });
    await waitFor(() => expect(activeName(container)).toBe("f3.txt"));

    await fireEvent.keyDown(document, { key: "ArrowRight" });
    await waitFor(() => expect(activeName(container)).toBe("f4.txt"));

    await fireEvent.keyDown(document, { key: "ArrowUp" });
    await waitFor(() => expect(activeName(container)).toBe("f1.txt"));
  });

  it("ignores arrow keys while typing in an input", async () => {
    setDetailsVisible(false);
    getFilesMock.mockResolvedValue(files());

    const { findByPlaceholderText, findByText, container } =
      renderWithProviders(FileBrowser as any);
    await findByText("a.txt");
    await waitFor(() => expect(activeName(container)).toBe("a.txt"));

    const input = await findByPlaceholderText("搜索名称、扩展名或路径");
    await fireEvent.keyDown(input, { key: "ArrowDown" });

    expect(activeName(container)).toBe("a.txt");
  });
});

describe("FileBrowser.vue directory tree", () => {
  it("shows the tree on wide screens and navigates from it", async () => {
    // 宽屏：仅 min-width 查询为真
    vi.stubGlobal("matchMedia", (q: string) => ({
      matches: q.includes("min-width"),
      media: q,
      onchange: null,
      addListener: () => {},
      removeListener: () => {},
      addEventListener: () => {},
      removeEventListener: () => {},
      dispatchEvent: () => false,
    }));
    setDetailsVisible(false);
    getFilesMock.mockResolvedValue([
      {
        id: "docs",
        name: "docs",
        path: "docs",
        kind: "directory",
        created_at: "2026-04-10T00:00:00.000Z",
        updated_at: "2026-04-10T00:00:00.000Z",
      },
    ]);

    const { container } = renderWithProviders(FileBrowser as any);

    // 目录树与文件列表里都会出现目录名，这里限定在树内查找
    await waitFor(() => {
      const tree = container.querySelector(".directory-tree");
      expect(tree).not.toBeNull();
      expect(within(tree as HTMLElement).queryByText("docs")).not.toBeNull();
    });

    const tree = container.querySelector(".directory-tree") as HTMLElement;
    await fireEvent.click(within(tree).getByText("docs"));

    await waitFor(() => {
      expect(getFilesPageMock).toHaveBeenCalledWith("docs", {
        commit: undefined,
        limit: 200,
        offset: 0,
      });
    });
  });
});

describe("FileBrowser.vue details dialog", () => {
  it("opens metadata from the context menu on any entry", async () => {
    setDetailsVisible(false);
    getFilesMock.mockResolvedValue([
      {
        id: "readme",
        name: "readme.md",
        path: "readme.md",
        kind: "file",
        size_bytes: 4096,
        created_at: "2026-04-10T00:00:00.000Z",
        updated_at: "2026-04-10T00:00:00.000Z",
      },
    ]);

    const { findByRole, findByText, container } = renderWithProviders(
      FileBrowser as any,
    );
    const name = await findByText("readme.md");
    await fireEvent.contextMenu(name.closest("tr")!);

    await fireEvent.click(await findByRole("menuitem", { name: "详细信息" }));

    await waitFor(() => {
      expect(container.ownerDocument.body.textContent).toContain(
        "详细信息: readme.md",
      );
    });
    const dialog = container.ownerDocument.querySelector(".modal.is-active");
    expect(dialog?.textContent).toContain("4 KB");
    expect(dialog?.textContent).toContain("文本文档");
  });
});

describe("FileBrowser.vue window file drop", () => {
  function dispatchDrop(files: File[]) {
    const event = new Event("drop", { cancelable: true, bubbles: true });
    Object.assign(event, {
      dataTransfer: { types: ["Files"], files, dropEffect: "" },
    });
    window.dispatchEvent(event);
  }

  it("queues files dropped onto the window", async () => {
    getFilesMock.mockResolvedValue([]);

    const { findAllByText, findByText, container } = renderWithProviders(
      FileBrowser as any,
    );
    await findByText("此文件夹为空");

    dispatchDrop([
      new File(["a"], "拖入-1.txt", { type: "text/plain" }),
      new File(["b"], "拖入-2.txt", { type: "text/plain" }),
    ]);

    // 上传对话框自动打开并带上这两个文件
    await findAllByText("拖入-1.txt");
    expect(container.textContent).toContain("拖入-2.txt");
  });

  it("shows the overlay only while a file drag is in progress", async () => {
    getFilesMock.mockResolvedValue([]);
    const { findByText, container } = renderWithProviders(FileBrowser as any);
    await findByText("此文件夹为空");

    const dragEnter = new Event("dragenter", { bubbles: true });
    Object.assign(dragEnter, {
      dataTransfer: { types: ["Files"], files: [], dropEffect: "" },
    });
    window.dispatchEvent(dragEnter);
    await waitFor(() => {
      expect(container.ownerDocument.body.textContent).toContain(
        "松开即可上传",
      );
    });

    const dragLeave = new Event("dragleave", { bubbles: true });
    Object.assign(dragLeave, {
      dataTransfer: { types: ["Files"], files: [], dropEffect: "" },
    });
    window.dispatchEvent(dragLeave);
    await waitFor(() => {
      expect(container.ownerDocument.body.textContent).not.toContain(
        "松开即可上传",
      );
    });
  });
});

describe("FileBrowser.vue details panel", () => {
  const files = [
    {
      id: "docs",
      name: "docs",
      path: "docs",
      kind: "directory",
      created_at: "2026-04-10T00:00:00.000Z",
      updated_at: "2026-04-10T00:00:00.000Z",
    },
    {
      id: "readme",
      name: "readme.md",
      path: "readme.md",
      kind: "file",
      size_bytes: 2048,
      created_at: "2026-04-10T00:00:00.000Z",
      updated_at: "2026-04-10T00:00:00.000Z",
    },
  ];

  it("shows metadata and actions for the active entry", async () => {
    setDetailsVisible(true);
    getFilesMock.mockResolvedValue(files);

    const { findByText, container } = renderWithProviders(FileBrowser as any);
    await findByText("readme.md");

    const panel = container.querySelector(".desktop-details");
    expect(panel).not.toBeNull();
    // 默认活动条目是第一个真实条目（文件夹）
    await waitFor(() => {
      expect(
        panel?.querySelector(".desktop-details-name")?.textContent,
      ).toContain("docs");
    });
    expect(panel?.textContent).toContain("文件夹");
    const actions = Array.from(
      panel?.querySelectorAll(".desktop-details-actions button") ?? [],
    ).map((button) => button.textContent?.trim());
    expect(actions).toContain("打开");
    expect(actions).toContain("删除");
  });

  it("follows the active entry and can be hidden", async () => {
    setDetailsVisible(true);
    getFilesMock.mockResolvedValue(files);

    const { findByText, getByLabelText, container } = renderWithProviders(
      FileBrowser as any,
    );
    await findByText("readme.md");
    const panel = () => container.querySelector(".desktop-details");

    await fireEvent.click(await findByText("readme.md"));
    await waitFor(() => {
      expect(
        panel()?.querySelector(".desktop-details-name")?.textContent,
      ).toContain("readme.md");
    });

    await fireEvent.click(getByLabelText("隐藏详细信息"));
    await waitFor(() => expect(panel()).toBeNull());

    await fireEvent.click(getByLabelText("显示详细信息"));
    await waitFor(() => expect(panel()).not.toBeNull());
  });
});

describe("FileBrowser.vue preview copy", () => {
  function typeScriptFile() {
    return [
      {
        id: "main",
        name: "main.ts",
        path: "main.ts",
        kind: "file",
        size_bytes: 12,
        created_at: "2026-04-10T00:00:00.000Z",
        updated_at: "2026-04-10T00:00:00.000Z",
      },
    ];
  }

  it("copies the previewed source code to the clipboard", async () => {
    const writeText = vi.fn(async () => {});
    Object.defineProperty(navigator, "clipboard", {
      value: { writeText },
      configurable: true,
    });
    getFilesMock.mockResolvedValue(typeScriptFile());
    getFileContentMock.mockResolvedValue(
      new Blob(["const answer: number = 42;\n"]),
    );

    const { findByText, findByRole } = renderWithProviders(FileBrowser as any);

    const name = await findByText("main.ts");
    await fireEvent.dblClick(name.closest("tr")!);

    const copyButton = await findByRole("button", { name: /复制/ });
    await fireEvent.click(copyButton);

    await waitFor(() => {
      expect(writeText).toHaveBeenCalledWith("const answer: number = 42;\n");
    });
    await findByText("已复制");
  });

  it("does not offer copying for binary previews", async () => {
    getFilesMock.mockResolvedValue([
      {
        id: "img",
        name: "photo.png",
        path: "photo.png",
        kind: "file",
        size_bytes: 4,
        created_at: "2026-04-10T00:00:00.000Z",
        updated_at: "2026-04-10T00:00:00.000Z",
      },
    ]);
    getFileContentMock.mockResolvedValue(new Blob(["png"]));

    const { findByText, queryByRole } = renderWithProviders(FileBrowser as any);

    const name = await findByText("photo.png");
    await fireEvent.dblClick(name.closest("tr")!);

    await waitFor(() => {
      expect(queryByRole("button", { name: /复制/ })).toBeNull();
    });
  });
});

describe("FileBrowser.vue preview retry", () => {
  it("retries a failed preview", async () => {
    getFilesMock.mockResolvedValue([
      {
        id: "a",
        name: "a.txt",
        path: "a.txt",
        kind: "file",
        size_bytes: 1,
        created_at: "2026-04-10T00:00:00.000Z",
        updated_at: "2026-04-10T00:00:00.000Z",
      },
    ]);
    getFileContentMock.mockRejectedValueOnce(new Error("预览加载失败"));
    getFileContentMock.mockResolvedValueOnce(new Blob(["hello preview"]));

    const { findByRole, findByText } = renderWithProviders(FileBrowser as any);

    const name = await findByText("a.txt");
    await fireEvent.dblClick(name.closest("tr")!);

    await findByText("预览加载失败");
    await fireEvent.click(await findByRole("button", { name: /重试/ }));

    await waitFor(() => {
      expect(getFileContentMock).toHaveBeenCalledTimes(2);
    });
  });
});

describe("FileBrowser.vue keyboard shortcuts help", () => {
  function fileInfo(name: string) {
    return {
      id: name,
      name,
      path: name,
      kind: "file" as const,
      size_bytes: 12,
      created_at: "2026-04-10T00:00:00.000Z",
      updated_at: "2026-04-10T00:00:00.000Z",
    };
  }

  it("opens the shortcut panel with ? and closes it again", async () => {
    setDetailsVisible(false);
    getFilesMock.mockResolvedValue([fileInfo("a.txt")]);

    const { findByText, container } = renderWithProviders(FileBrowser as any);
    await findByText("a.txt");

    expect(container.querySelector(".shortcuts")).toBeNull();

    await fireEvent.keyDown(document, { key: "?" });
    await waitFor(() =>
      expect(container.querySelector(".shortcuts")).not.toBeNull(),
    );
    expect(
      container.querySelector(".modal.is-active .modal-card-title")
        ?.textContent,
    ).toBe("键盘快捷键");

    await fireEvent.keyDown(document, { key: "?" });
    await waitFor(() =>
      expect(container.querySelector(".shortcuts")).toBeNull(),
    );
  });

  it("opens the panel from the status bar hint", async () => {
    setDetailsVisible(false);
    getFilesMock.mockResolvedValue([fileInfo("a.txt")]);

    const { findByText, container } = renderWithProviders(FileBrowser as any);
    await findByText("a.txt");

    const hint = container.querySelector<HTMLButtonElement>(
      ".desktop-status-shortcuts",
    );
    expect(hint).not.toBeNull();
    await fireEvent.click(hint!);
    await waitFor(() =>
      expect(container.querySelector(".shortcuts")).not.toBeNull(),
    );
  });

  it("does not hijack ? while typing in a field", async () => {
    setDetailsVisible(false);
    getFilesMock.mockResolvedValue([fileInfo("a.txt")]);

    const { findByText, container } = renderWithProviders(FileBrowser as any);
    await findByText("a.txt");

    const input = container.querySelector<HTMLInputElement>(
      ".desktop-search-control",
    )!;
    await fireEvent.keyDown(input, { key: "?" });
    await new Promise((resolve) => setTimeout(resolve, 20));
    expect(container.querySelector(".shortcuts")).toBeNull();
  });
});

describe("FileBrowser.vue keyboard navigation extras", () => {
  function files(count: number) {
    return Array.from({ length: count }, (_, index) => ({
      id: `f${index}`,
      name: `文件-${String(index).padStart(3, "0")}.txt`,
      path: `文件-${String(index).padStart(3, "0")}.txt`,
      kind: "file" as const,
      size_bytes: 10,
      created_at: "2026-04-10T00:00:00.000Z",
      updated_at: "2026-04-10T00:00:00.000Z",
    }));
  }

  function activePath(container: Element): string | null {
    return (
      container
        .querySelector<HTMLElement>(
          "tr.desktop-file-row.is-row-selected, tr.desktop-file-row.is-active",
        )
        ?.getAttribute("data-vfiles-path") ?? null
    );
  }

  it("jumps to an entry by typing its name prefix and shows the prefix", async () => {
    setDetailsVisible(false);
    getFilesMock.mockResolvedValue([
      { ...files(1)[0], name: "alpha.txt", path: "alpha.txt" },
      { ...files(1)[0], name: "beta.txt", path: "beta.txt" },
      { ...files(1)[0], name: "报告.md", path: "报告.md" },
    ]);

    const { findByText, container } = renderWithProviders(FileBrowser as any);
    await findByText("alpha.txt");

    await fireEvent.keyDown(document, { key: "b" });
    await waitFor(() => expect(activePath(container)).toBe("beta.txt"));
    expect(
      container.querySelector(".desktop-status-typeahead")?.textContent,
    ).toContain("b");

    // Esc 清空定位前缀
    await fireEvent.keyDown(document, { key: "Escape" });
    await waitFor(() =>
      expect(container.querySelector(".desktop-status-typeahead")).toBeNull(),
    );
  });

  it("ignores typed characters that match nothing", async () => {
    setDetailsVisible(false);
    getFilesMock.mockResolvedValue(files(3));

    const { findByText, container } = renderWithProviders(FileBrowser as any);
    await findByText("文件-000.txt");

    await fireEvent.keyDown(document, { key: "z" });
    await new Promise((resolve) => setTimeout(resolve, 20));
    expect(container.querySelector(".desktop-status-typeahead")).toBeNull();
  });

  it("moves by a page with PageDown and toggles selection with Ctrl+Space", async () => {
    setDetailsVisible(false);
    getFilesMock.mockResolvedValue(files(30));

    const { findByText, container } = renderWithProviders(FileBrowser as any);
    await findByText("文件-000.txt");

    // 无布局环境下按 10 行兜底：Home 复位后 PageDown/PageUp 各走 10 行
    await fireEvent.keyDown(document, { key: "Home" });
    await waitFor(() => expect(activePath(container)).toBe("文件-000.txt"));

    await fireEvent.keyDown(document, { key: "PageDown" });
    await waitFor(() => expect(activePath(container)).toBe("文件-010.txt"));

    await fireEvent.keyDown(document, { key: "PageUp" });
    await waitFor(() => expect(activePath(container)).toBe("文件-000.txt"));

    // Ctrl+Space 选中高亮行并进入批量模式（r124 ✓ 空格让位快速预览（云盘惯例））
    // 自构造 KeyboardEvent（fireEvent 修饰键回显存疑 → dispatch 式稳）
    document.dispatchEvent(
      new KeyboardEvent("keydown", { key: " ", ctrlKey: true, bubbles: true }),
    );
    await nextTick();
    await waitFor(() =>
      expect(
        container.querySelector("tr.desktop-file-row.is-row-selected"),
      ).not.toBeNull(),
    );
    // 再按一次取消
    document.dispatchEvent(
      new KeyboardEvent("keydown", { key: " ", ctrlKey: true, bubbles: true }),
    );
    await nextTick();
    await waitFor(() =>
      expect(
        container.querySelector("tr.desktop-file-row.is-row-selected"),
      ).toBeNull(),
    );
  });
});

describe("FileBrowser.vue grid paging", () => {
  function gridFiles(count: number) {
    return Array.from({ length: count }, (_, index) => ({
      id: `g${index}`,
      name: `文件-${String(index).padStart(3, "0")}.txt`,
      path: `文件-${String(index).padStart(3, "0")}.txt`,
      kind: "file" as const,
      size_bytes: 10,
      created_at: "2026-04-10T00:00:00.000Z",
      updated_at: "2026-04-10T00:00:00.000Z",
    }));
  }

  it("moves a full row with arrows and a page with PageDown", async () => {
    setDetailsVisible(false);
    getFilesMock.mockResolvedValue(gridFiles(30));

    const { findByText, container } = renderWithProviders(FileBrowser as any);
    await findByText("文件-000.txt");

    // 切到网格视图
    const { useFileViewStore } = await import("../src/stores/fileView.store");
    useFileViewStore().setMode("grid");
    await waitFor(() =>
      expect(container.querySelectorAll(".file-card").length).toBeGreaterThan(
        3,
      ),
    );

    // jsdom 无布局：手动构造 3 列、行高 200、可视高度 600 的布局
    const cards = Array.from(
      container.querySelectorAll<HTMLElement>(".file-card"),
    );
    const columns = 3;
    const rowHeight = 200;
    const shell = container.querySelector<HTMLElement>(".desktop-list-shell")!;
    cards.forEach((card, index) => {
      const row = Math.floor(index / columns);
      Object.defineProperty(card, "offsetTop", { value: row * rowHeight });
      card.getBoundingClientRect = () =>
        ({
          top: row * rowHeight,
          height: rowHeight - 20,
          bottom: row * rowHeight + rowHeight - 20,
          left: 0,
          right: 100,
          width: 100,
          x: 0,
          y: row * rowHeight,
          toJSON: () => ({}),
        }) as DOMRect;
    });
    Object.defineProperty(shell, "clientHeight", { value: 600 });

    const activeIndex = () =>
      cards.findIndex(
        (card) =>
          card.classList.contains("file-card--active") ||
          card.classList.contains("file-card--selected"),
      );

    await fireEvent.keyDown(document, { key: "Home" });
    await waitFor(() => expect(activeIndex()).toBe(0));

    // ↓ 按整行移动（3 列）
    await fireEvent.keyDown(document, { key: "ArrowDown" });
    await waitFor(() => expect(activeIndex()).toBe(columns));

    // PageDown 按「可视行数 × 列数」= 3 行 × 3 列 = 9
    await fireEvent.keyDown(document, { key: "PageDown" });
    await waitFor(() => expect(activeIndex()).toBe(columns + 9));
  });
});

describe("FileBrowser.vue ownership transfer", () => {
  function transferFile(name: string) {
    return {
      id: name,
      name,
      path: name,
      kind: "file" as const,
      size_bytes: 12,
      created_at: "2026-04-10T00:00:00.000Z",
      updated_at: "2026-04-10T00:00:00.000Z",
    };
  }

  it("opens the transfer dialog from the row context menu and calls the service", async () => {
    setDetailsVisible(false);
    getFilesMock.mockResolvedValue([transferFile("交接.txt")]);
    listTransferTargetsMock.mockReset();
    transferOwnershipMock.mockReset();
    listTransferTargetsMock.mockResolvedValue([
      { id: "u-1", username: "alice" },
    ]);
    transferOwnershipMock.mockResolvedValue({
      transferred: 1,
      target_username: "alice",
    });

    const { findByText } = renderWithProviders(FileBrowser as any);
    await findByText("交接.txt");

    // 右键菜单 → 转移所有权
    await fireEvent.contextMenu(
      document.querySelector(
        'tr.desktop-file-row[data-vfiles-path="交接.txt"]',
      )!,
    );
    const menuItem = (await screen.findAllByText("转移所有权")).find((node) =>
      node.closest('[role="menuitem"]'),
    );
    await fireEvent.click(menuItem!);

    // 对话框出现并列出目标用户
    await waitFor(() =>
      expect(screen.getByText("接收用户")).toBeInTheDocument(),
    );
    await waitFor(() => expect(screen.getByText("alice")).toBeInTheDocument());

    await fireEvent.click(screen.getByText("alice"));
    await fireEvent.click(
      screen
        .getAllByText("转移所有权")
        .map((node) => node.closest("button"))
        .find((button) => button !== null)!,
    );

    await waitFor(() =>
      expect(transferOwnershipMock).toHaveBeenCalledWith(
        ["交接.txt"],
        "u-1",
        undefined,
      ),
    );
  });
});

describe("FileBrowser.vue delete confirmation", () => {
  function deleteFixture(name: string, kind: "file" | "directory" = "file") {
    return {
      id: name,
      name,
      path: name,
      kind,
      size_bytes: 10,
      created_at: "2026-04-10T00:00:00.000Z",
      updated_at: "2026-04-10T00:00:00.000Z",
    };
  }

  function lastConfirmCall(): any {
    return vi.mocked(confirmDialog).mock.calls[
      vi.mocked(confirmDialog).mock.calls.length - 1
    ][0];
  }

  async function openDeleteFromContextMenu(path: string, container: Element) {
    const row = container.querySelector(
      `tr.desktop-file-row[data-vfiles-path="${path}"]`,
    );
    expect(row, `row ${path} should exist`).not.toBeNull();
    await fireEvent.contextMenu(row!);
    const findDeleteItem = () =>
      Array.from(document.querySelectorAll('[role="menuitem"]')).find((item) =>
        item.textContent?.includes("删除"),
      );
    await waitFor(() => expect(findDeleteItem()).toBeDefined());
    await fireEvent.click(findDeleteItem()!);
  }

  it("requires confirmation for a single file delete and cancels when declined", async () => {
    setDetailsVisible(false);
    getFilesMock.mockResolvedValue([deleteFixture("单删.txt")]);
    deleteFileMock.mockClear();
    const confirmMock = vi.mocked(confirmDialog);
    confirmMock.mockReset();

    const { findByText, container } = renderWithProviders(FileBrowser as any);
    await findByText("单删.txt");

    // 拒绝确认 → 不应发出删除请求
    confirmMock.mockResolvedValueOnce(false);
    await openDeleteFromContextMenu("单删.txt", container);
    await waitFor(() => expect(confirmMock).toHaveBeenCalledTimes(1));
    await new Promise((resolve) => setTimeout(resolve, 30));
    expect(deleteFileMock).not.toHaveBeenCalled();

    // 接受确认 → 才真正删除
    confirmMock.mockResolvedValueOnce(true);
    await openDeleteFromContextMenu("单删.txt", container);
    await waitFor(() =>
      expect(deleteFileMock).toHaveBeenCalledWith(
        "单删.txt",
        "删除文件: 单删.txt",
      ),
    );
  });

  it("warns that a directory delete includes its contents", async () => {
    setDetailsVisible(false);
    getFilesMock.mockResolvedValue([deleteFixture("项目目录", "directory")]);
    deleteFileMock.mockClear();
    const confirmMock = vi.mocked(confirmDialog);
    confirmMock.mockReset();
    confirmMock.mockResolvedValueOnce(false);

    const { findByText, container } = renderWithProviders(FileBrowser as any);
    await findByText("项目目录");

    await openDeleteFromContextMenu("项目目录", container);
    await waitFor(() => expect(confirmMock).toHaveBeenCalledTimes(1));

    const options = lastConfirmCall();
    expect(options.title).toBe("删除目录");
    expect(options.message).toContain("项目目录");
    expect(options.message).toContain("全部内容");
    expect(options.message).toContain("不可撤销");
    expect(options.danger).toBe(true);
    expect(options.confirmText).toBe("删除");
    expect(deleteFileMock).not.toHaveBeenCalled();
  });

  it("asks before deleting via the details panel too", async () => {
    setDetailsVisible(true);
    getFilesMock.mockResolvedValue([deleteFixture("面板删除.txt")]);
    deleteFileMock.mockClear();
    const confirmMock = vi.mocked(confirmDialog);
    confirmMock.mockReset();
    confirmMock.mockResolvedValueOnce(false);

    const { container } = renderWithProviders(FileBrowser as any);
    // 详情面板开启时名称会出现两次（行 + 面板标题），按行存在来等加载完成
    await waitFor(() =>
      expect(
        container.querySelector(
          'tr.desktop-file-row[data-vfiles-path="面板删除.txt"]',
        ),
      ).not.toBeNull(),
    );

    // 详情面板操作区的删除按钮（同一套确认逻辑）
    const buttons = Array.from(
      document.querySelectorAll<HTMLButtonElement>("button"),
    ).filter((button) => button.textContent?.trim() === "删除");
    expect(buttons.length).toBeGreaterThan(0);

    await fireEvent.click(buttons[buttons.length - 1]!);
    await waitFor(() => expect(confirmMock).toHaveBeenCalledTimes(1));
    await new Promise((resolve) => setTimeout(resolve, 30));
    expect(deleteFileMock).not.toHaveBeenCalled();
  });
});

describe("FileBrowser.vue drag lift", () => {
  it("dims the source row while dragging and restores it on dragend", async () => {
    setDetailsVisible(false);
    getFilesMock.mockResolvedValue([
      {
        id: "drag-me.txt",
        name: "drag-me.txt",
        path: "drag-me.txt",
        kind: "file" as const,
        size_bytes: 10,
        created_at: "2026-04-10T00:00:00.000Z",
        updated_at: "2026-04-10T00:00:00.000Z",
      },
    ]);

    const { findByText, container } = renderWithProviders(FileBrowser as any);
    await findByText("drag-me.txt");

    const row = container.querySelector('tr[data-vfiles-path="drag-me.txt"]')!;
    expect(row.classList.contains("is-dragging")).toBe(false);

    fireEvent.dragStart(row);
    await waitFor(() =>
      expect(row.classList.contains("is-dragging")).toBe(true),
    );

    fireEvent.dragEnd(row);
    await waitFor(() =>
      expect(row.classList.contains("is-dragging")).toBe(false),
    );
  });
});

describe("FileBrowser.vue drop hint", () => {
  it("shows the target folder name while a directory is dragged over and hides it after", async () => {
    setDetailsVisible(false);
    getFilesMock.mockResolvedValue([
      {
        id: "项目",
        name: "项目",
        path: "项目",
        kind: "directory" as const,
        size_bytes: 0,
        created_at: "2026-04-10T00:00:00.000Z",
        updated_at: "2026-04-10T00:00:00.000Z",
      },
      {
        id: "a.txt",
        name: "a.txt",
        path: "a.txt",
        kind: "file" as const,
        size_bytes: 10,
        created_at: "2026-04-10T00:00:00.000Z",
        updated_at: "2026-04-10T00:00:00.000Z",
      },
    ]);

    const { findByText, container } = renderWithProviders(FileBrowser as any);
    await findByText("项目");
    // 静止状态：无提示、目录名可见
    expect(container.querySelector(".desktop-drop-hint")).toBeNull();

    const dirRow = container.querySelector('tr[data-vfiles-path="项目"]')!;
    const fileRow = container.querySelector('tr[data-vfiles-path="a.txt"]')!;

    fireEvent.dragOver(dirRow);
    await waitFor(() =>
      expect(
        container.querySelector(".desktop-drop-hint")?.textContent,
      ).toContain("移动到「项目」"),
    );

    fireEvent.dragLeave(dirRow);
    fireEvent.dragEnd(fileRow);
    await waitFor(() =>
      expect(container.querySelector(".desktop-drop-hint")).toBeNull(),
    );
    // 名称不受提示影响，始终可见
    expect(dirRow.textContent).toContain("项目");
  });

  it("shows the type-ahead HUD while locating by name prefix (r77)", async () => {
    setDetailsVisible(false);
    getFilesMock.mockResolvedValue([
      {
        id: "甲乙.txt",
        name: "甲乙.txt",
        path: "甲乙.txt",
        kind: "file" as const,
        size_bytes: 1,
        created_at: "2026-04-10T00:00:00.000Z",
        updated_at: "2026-04-10T00:00:00.000Z",
      },
      {
        id: "丙丁戊.txt",
        name: "丙丁戊.txt",
        path: "丙丁戊.txt",
        kind: "file" as const,
        size_bytes: 1,
        created_at: "2026-04-10T00:00:00.000Z",
        updated_at: "2026-04-10T00:00:00.000Z",
      },
    ]);
    const { container } = renderWithProviders(FileBrowser as any);
    await waitFor(() =>
      expect(
        container.querySelectorAll("tr.desktop-file-row").length,
      ).toBeGreaterThan(0),
    );

    // 可打印字符 → 名称前缀定位 + 状态栏 HUD（此前零测试护持 ✗）
    // 挂点 = document（onDocKeydown ✓ 工具语：window ≠ document 别想当然）
    await fireEvent.keyDown(document, { key: "丙" });
    await waitFor(() => {
      const hud = container.querySelector(".desktop-status-typeahead");
      expect(hud).not.toBeNull();
      expect(hud!.textContent).toContain("定位");
      expect(hud!.textContent).toContain("丙");
    });
    // 中央浮动 HUD（r78 Finder 式回执 ✓ 与状态栏并存）
    await waitFor(() => {
      const pill = container.ownerDocument.querySelector(
        ".desktop-typeahead-hud",
      );
      expect(pill).not.toBeNull();
      expect(pill!.textContent!.trim()).toBe("丙");
    });
    const active = container.querySelector('tr[data-vfiles-path="丙丁戊.txt"]');
    expect(active).not.toBeNull();
  });

  it("shows a cursor-following drag chip while dragging", async () => {
    setDetailsVisible(false);
    getFilesMock.mockResolvedValue([
      {
        id: "项目",
        name: "项目",
        path: "项目",
        kind: "directory" as const,
        size_bytes: 0,
        created_at: "2026-04-10T00:00:00.000Z",
        updated_at: "2026-04-10T00:00:00.000Z",
      },
      {
        id: "a.txt",
        name: "a.txt",
        path: "a.txt",
        kind: "file" as const,
        size_bytes: 3,
        created_at: "2026-04-10T00:00:00.000Z",
        updated_at: "2026-04-10T00:00:00.000Z",
      },
    ]);
    const { container } = renderWithProviders(FileBrowser as any);
    await waitFor(() =>
      expect(
        container.querySelectorAll("tr.desktop-file-row").length,
      ).toBeGreaterThan(0),
    );
    const fileRow = container.querySelectorAll(
      "tr.desktop-file-row",
    )[1] as HTMLElement;

    fireEvent.dragStart(fileRow);
    await waitFor(() => {
      const chip = document.querySelector(".desktop-drag-chip");
      expect(chip).not.toBeNull();
      expect(chip!.textContent).toContain("移动");
    });

    // dragover 实时驱动位置（光标右下 14px 偏移）——
    // jsdom 无 DragEvent 构造：用 MouseEvent 冒充 dragover（坐标 init 可用 ✓）
    // jsdom 无 elementFromPoint：桩到目录行 → 两段式 chip 切「放入 目标名」
    const dirRow = Array.from(
      container.querySelectorAll("tr.desktop-file-row"),
    ).find((r) => r.querySelector("a.desktop-name-link")) as HTMLElement;
    (document as any).elementFromPoint = () => dirRow;
    document.dispatchEvent(
      new MouseEvent("dragover", {
        clientX: 300,
        clientY: 200,
        bubbles: true,
      }),
    );
    await waitFor(() => {
      const chip = document.querySelector(".desktop-drag-chip") as HTMLElement;
      expect(chip.textContent).toContain("放入");
      expect(chip.className).toContain("is-over-target");
    });

    // 非法目标（拖自身行）→ 禁止态
    (document as any).elementFromPoint = () => fileRow;
    document.dispatchEvent(
      new MouseEvent("dragover", {
        clientX: 300,
        clientY: 200,
        bubbles: true,
      }),
    );
    await waitFor(() => {
      const chip = document.querySelector(".desktop-drag-chip") as HTMLElement;
      expect(chip.textContent).toContain("不能放到这里");
      expect(chip.className).toContain("is-invalid");
    });

    // 离开落点（桩到空白 body）→ 回「移动 …」（桩回拖拽行自身 = 仍是禁止态 ✗ 教训）
    (document as any).elementFromPoint = () => document.body;
    document.dispatchEvent(
      new MouseEvent("dragover", {
        clientX: 300,
        clientY: 200,
        bubbles: true,
      }),
    );
    await waitFor(() => {
      const chip = document.querySelector(".desktop-drag-chip") as HTMLElement;
      expect(chip.textContent).toContain("移动");
    });
    document.dispatchEvent(
      new MouseEvent("dragover", {
        clientX: 300,
        clientY: 200,
        bubbles: true,
      }),
    );
    await waitFor(() => {
      const chip = document.querySelector(".desktop-drag-chip") as HTMLElement;
      expect(chip.style.left).toBe("314px");
      expect(chip.style.top).toBe("214px");
    });

    fireEvent.dragEnd(fileRow);
    await waitFor(() =>
      expect(document.querySelector(".desktop-drag-chip")).toBeNull(),
    );
  });


  it("sets a dynamic document title (r147)", async () => {
    renderWithProviders(FileBrowser as any);
    await nextTick();
    // 根目录标题 = 产品默认（immediate watch ✓ 渲染即设、无需行渲染）
    expect(document.title).toContain("VFiles");
  });
});
