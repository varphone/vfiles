import { fireEvent, render, screen, waitFor } from "@testing-library/vue";
import { beforeEach, describe, expect, it, vi } from "vitest";
import DirectoryTree from "../src/components/file-browser/DirectoryTree.vue";
import type { FileInfo } from "../src/types";

const { getFilesPageMock } = vi.hoisted(() => ({
  getFilesPageMock: vi.fn(),
}));

vi.mock("../src/services/files.service", () => ({
  SEARCH_PAGE_SIZE: 100,
  filesService: { getFilesPage: getFilesPageMock },
}));

/** 与真实接口一致：`path` 是完整路径，`name` 只是最后一段。 */
function entry(path: string, kind: "file" | "directory"): FileInfo {
  return {
    id: path,
    name: path.split("/").pop() ?? path,
    path,
    kind,
    created_at: "2026-01-01T00:00:00.000Z",
  };
}

function page(items: FileInfo[]) {
  return { items, total: items.length, limit: 200, offset: 0, has_more: false };
}

/** 目录结构：root -> docs/图片；docs -> api/guides */
function installTree() {
  getFilesPageMock.mockImplementation(async (path: string) => {
    if (path === "") {
      return page([
        entry("图片", "directory"),
        entry("docs", "directory"),
        entry("root.txt", "file"),
      ]);
    }
    if (path === "docs") {
      return page([
        entry("docs/api", "directory"),
        entry("docs/guides", "directory"),
      ]);
    }
    if (path === "docs/api") {
      return page([entry("docs/api/nested.txt", "file")]);
    }
    return page([]);
  });
}

function renderTree(props: Record<string, unknown> = {}) {
  return render(DirectoryTree as any, {
    props: { currentPath: "", dragging: false, ...props },
  });
}

describe("DirectoryTree.vue", () => {
  beforeEach(() => {
    getFilesPageMock.mockReset();
    installTree();
  });

  it("lists root directories but not files", async () => {
    renderTree();

    await waitFor(() => {
      expect(screen.getByText("docs")).toBeInTheDocument();
    });
    expect(screen.getByText("图片")).toBeInTheDocument();
    expect(screen.queryByText("root.txt")).toBeNull();
    // 目录名按本地化顺序排序
    const names = screen
      .getAllByText(/docs|图片/)
      .map((el) => el.textContent?.trim());
    expect(names).toEqual(["图片", "docs"]);
  });

  it("loads children on expand and nests them", async () => {
    renderTree();
    await waitFor(() => expect(screen.getByText("docs")).toBeInTheDocument());

    await fireEvent.click(screen.getByLabelText("展开 docs"));

    await waitFor(() => {
      expect(screen.getByText("api")).toBeInTheDocument();
    });
    expect(screen.getByText("guides")).toBeInTheDocument();
    expect(getFilesPageMock).toHaveBeenCalledWith("docs", {
      limit: 200,
      offset: 0,
    });

    // 收起后子级消失
    await fireEvent.click(screen.getByLabelText("收起 docs"));
    await waitFor(() => {
      expect(screen.queryByText("api")).toBeNull();
    });
  });

  it("emits navigate with the directory path", async () => {
    const { emitted } = renderTree();
    await waitFor(() => expect(screen.getByText("docs")).toBeInTheDocument());

    await fireEvent.click(screen.getByText("docs"));

    expect(emitted()["navigate"]?.[0]).toEqual(["docs"]);
  });

  it("highlights the current directory and reveals its ancestors", async () => {
    renderTree({ currentPath: "docs/api" });

    await waitFor(() => {
      const active = document.querySelector(".directory-tree-row.is-active");
      expect(active?.textContent?.trim()).toBe("api");
    });
    // 祖先目录被自动展开
    expect(screen.getByText("guides")).toBeInTheDocument();
  });

  it("refreshes loaded levels when the refresh key changes", async () => {
    const { rerender } = renderTree({ refreshKey: 0 });
    await waitFor(() => expect(screen.getByText("docs")).toBeInTheDocument());
    // 展开 docs，让第二层进入缓存
    await fireEvent.click(screen.getByLabelText("展开 docs"));
    await waitFor(() => expect(screen.getByText("api")).toBeInTheDocument());

    // 服务端新增了一个目录：刷新后两层都应更新
    getFilesPageMock.mockImplementation(async (path: string) => {
      if (path === "") {
        return page([
          entry("图片", "directory"),
          entry("docs", "directory"),
          entry("fresh", "directory"),
        ]);
      }
      if (path === "docs") {
        return page([
          entry("docs/api", "directory"),
          entry("docs/guides", "directory"),
          entry("docs/new", "directory"),
        ]);
      }
      return page([]);
    });

    await rerender({ currentPath: "", dragging: false, refreshKey: 1 });

    await waitFor(() => expect(screen.getByText("fresh")).toBeInTheDocument());
    // 展开状态保留，第二层也刷新到最新（两层请求分别完成，需各自等待）
    await waitFor(() => expect(screen.getByText("new")).toBeInTheDocument());
    expect(screen.getByLabelText("收起 docs")).toBeInTheDocument();
  });

  it("keeps the current directory revealed after a refresh", async () => {
    const { rerender } = renderTree({
      currentPath: "docs/api",
      refreshKey: 0,
    });
    await waitFor(() => expect(screen.getByText("api")).toBeInTheDocument());

    await rerender({ currentPath: "docs/api", refreshKey: 2 });

    await waitFor(() =>
      expect(
        document
          .querySelector(".directory-tree-row.is-active")
          ?.textContent?.trim(),
      ).toBe("api"),
    );
  });

  it("only accepts drops while dragging", async () => {
    const { emitted, unmount } = renderTree({ dragging: true });
    await waitFor(() => expect(screen.getByText("docs")).toBeInTheDocument());

    const item = screen.getByText("docs").closest(".directory-tree-item")!;
    await fireEvent.dragOver(item);
    expect(item.className).toContain("is-drag-over");
    await fireEvent.drop(item);
    expect(emitted()["drop-on-folder"]?.[0]).toEqual(["docs"]);
    unmount();

    const idle = renderTree({ dragging: false });
    await waitFor(() => expect(screen.getByText("docs")).toBeInTheDocument());
    const idleItem = screen.getByText("docs").closest(".directory-tree-item")!;
    await fireEvent.drop(idleItem);
    expect(idle.emitted()["drop-on-folder"]).toBeUndefined();
  });
});

describe("DirectoryTree.vue drop hint", () => {
  beforeEach(() => {
    getFilesPageMock.mockReset();
    installTree();
  });

  it("shows the target folder name while dragging over a row and hides it on leave", async () => {
    renderTree({ dragging: true });

    await waitFor(() => {
      expect(screen.getByText("docs")).toBeInTheDocument();
    });
    const item = screen.getByText("docs").closest(".directory-tree-item")!;

    fireEvent.dragOver(item);
    await waitFor(() =>
      expect(item.querySelector(".desktop-drop-hint")?.textContent).toContain(
        "移动到「docs」",
      ),
    );
    expect(item.classList.contains("is-drag-over")).toBe(true);

    fireEvent.dragLeave(item);
    await waitFor(() =>
      expect(item.querySelector(".desktop-drop-hint")).toBeNull(),
    );
  });
});
