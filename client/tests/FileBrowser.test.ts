import { fireEvent, waitFor } from "@testing-library/vue";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { renderWithProviders } from "./renderWithProviders";
import FileBrowser from "../src/components/file-browser/FileBrowser.vue";

const { getFilesMock, searchFilesMock, deleteFileMock, getFileContentMock } =
  vi.hoisted(() => ({
    getFilesMock: vi.fn(async (): Promise<unknown[]> => []),
    searchFilesMock: vi.fn(async (): Promise<unknown[]> => []),
    deleteFileMock: vi.fn(async () => ({ success: true })),
    getFileContentMock: vi.fn(async () => new Blob(["hello preview"])),
  }));

vi.mock("../src/composables/dialog", () => ({
  confirmDialog: vi.fn(async () => true),
  promptDialog: vi.fn(async () => null),
}));

vi.mock("../src/services/files.service", () => ({
  filesService: {
    getFiles: getFilesMock,
    searchFiles: searchFilesMock,
    deleteFile: deleteFileMock,
    getFileContent: getFileContentMock,
  },
}));

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

describe("FileBrowser.vue", () => {
  beforeEach(() => {
    getFilesMock.mockReset();
    searchFilesMock.mockReset();
    deleteFileMock.mockReset();
    getFileContentMock.mockReset();
    getFilesMock.mockResolvedValue([]);
    searchFilesMock.mockResolvedValue([]);
    deleteFileMock.mockResolvedValue({ success: true });
    getFileContentMock.mockResolvedValue(new Blob(["hello preview"]));
    stubBrowserApis();
  });

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
