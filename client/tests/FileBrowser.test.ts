import { fireEvent, waitFor, within } from "@testing-library/vue";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { renderWithProviders } from "./renderWithProviders";
import FileBrowser from "../src/components/file-browser/FileBrowser.vue";

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

    await findByText("搜索结果：1 项（文件名）");
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
    expect(queryByText("搜索结果：1 项（文件名）")).not.toBeInTheDocument();
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

    await findByText("预览: a.txt");

    await fireEvent.keyDown(document, { key: "ArrowRight" });
    await findByText("预览: b.txt");

    await fireEvent.keyDown(document, { key: "ArrowLeft" });
    await findByText("预览: a.txt");
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

  /** 单选场景下高亮行即活动行；多选时请用 selectedNames。 */
  function activeName(container: Element) {
    return container
      .querySelector(".desktop-file-row.is-row-selected .desktop-name-text")
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
    expect(dialog?.textContent).toContain("4.0 KB");
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
