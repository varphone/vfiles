import { fireEvent, render, screen, waitFor } from "@testing-library/vue";
import { beforeEach, describe, expect, it, vi } from "vitest";
import MoveDialog from "../src/components/file-browser/MoveDialog.vue";
import type { FileInfo } from "../src/types";

const { getDirectoriesPageMock } = vi.hoisted(() => ({
  getDirectoriesPageMock: vi.fn(),
}));

vi.mock("../src/services/files.service", () => ({
  SEARCH_PAGE_SIZE: 100,
  MAX_MOVE_BATCH_ITEMS: 500,
  filesService: { getDirectoriesPage: getDirectoriesPageMock },
}));

function entry(overrides: Partial<FileInfo>): FileInfo {
  return {
    id: overrides.path ?? "id",
    name: overrides.name ?? "x",
    path: overrides.path ?? "x",
    kind: overrides.kind ?? "directory",
    created_at: "2026-09-20T10:00:00.000Z",
    updated_at: "2026-09-20T10:00:00.000Z",
    ...overrides,
  };
}

const rootEntries = [
  entry({ name: "图片", path: "图片" }),
  entry({ name: "文档", path: "文档" }),
];

function directoryPage(items: FileInfo[], total = items.length, offset = 0) {
  return {
    items,
    total,
    limit: 100,
    offset,
    has_more: offset + items.length < total,
  };
}

function renderDialog(overrides: Record<string, unknown> = {}) {
  return render(MoveDialog as any, {
    props: {
      isActive: true,
      items: [{ name: "报告.txt", path: "报告.txt", kind: "file" }],
      initialPath: "",
      ...overrides,
    },
  });
}

describe("MoveDialog.vue", () => {
  beforeEach(() => {
    getDirectoriesPageMock.mockReset();
    getDirectoriesPageMock.mockResolvedValue(directoryPage(rootEntries));
  });

  it("summarises the items and lists only directories", async () => {
    // 条目来自子目录，因此根目录是可用的目标
    renderDialog({
      items: [{ name: "报告.txt", path: "文档/报告.txt", kind: "file" }],
    });

    await screen.findByText("图片");
    // 待移动条目在摘要行展示
    expect(
      document.querySelector(".move-dialog-items-name")?.textContent?.trim(),
    ).toBe("报告.txt");
    // 列表只呈现目录
    const rows = Array.from(document.querySelectorAll(".move-dialog-row")).map(
      (row) => row.textContent?.trim(),
    );
    expect(rows).toEqual(["图片", "文档"]);
    expect(screen.getByRole("button", { name: /移动到根目录/ })).toBeEnabled();
  });

  it("navigates into a folder and offers to move there", async () => {
    const { emitted } = renderDialog({
      items: [{ name: "报告.txt", path: "文档/报告.txt", kind: "file" }],
    });

    await fireEvent.click(await screen.findByText("图片"));
    await waitFor(() =>
      expect(getDirectoriesPageMock).toHaveBeenLastCalledWith("图片", {
        limit: 100,
        offset: 0,
      }),
    );

    // 面包屑显示当前目标
    const crumbs = document.querySelectorAll(".move-dialog-crumb");
    expect(Array.from(crumbs).map((c) => c.textContent?.trim())).toEqual([
      "根目录",
      "图片",
    ]);

    const confirm = screen.getByRole("button", { name: /移动到「图片」/ });
    await fireEvent.click(confirm);
    expect(emitted()["confirm"]?.[0]).toEqual(["图片"]);
  });

  it("disables the confirm button when the target is the current parent", async () => {
    renderDialog({
      items: [{ name: "报告.txt", path: "文档/报告.txt", kind: "file" }],
      initialPath: "文档",
    });

    await waitFor(() =>
      expect(document.querySelector(".move-dialog-warning")).not.toBeNull(),
    );
    expect(
      document.querySelector(".move-dialog-warning")?.textContent,
    ).toContain("已经在当前目录");
    expect(
      screen.getByRole("button", { name: "移动到当前目录" }),
    ).toBeDisabled();
  });

  it("greys out folders that are being moved", async () => {
    renderDialog({
      items: [{ name: "图片", path: "图片", kind: "directory" }],
    });

    const disabled = await waitFor(() => {
      const row = document.querySelector<HTMLButtonElement>(
        ".move-dialog-row.is-disabled",
      );
      expect(row).not.toBeNull();
      return row!;
    });
    expect(disabled.textContent).toContain("图片");
    expect(disabled.textContent).toContain("待移动目录");
    expect(disabled.disabled).toBe(true);
  });

  it("shows the empty and error states inside the picker", async () => {
    getDirectoriesPageMock.mockResolvedValue(directoryPage([]));
    const { unmount } = renderDialog();
    expect(await screen.findByText("此处没有子文件夹")).toBeInTheDocument();
    unmount();

    getDirectoriesPageMock.mockRejectedValue(new Error("服务器内部错误"));
    renderDialog();
    expect(await screen.findByText("目录加载失败")).toBeInTheDocument();
    expect(screen.getByText("服务器内部错误")).toBeInTheDocument();
    expect(screen.getByText("重试")).toBeInTheDocument();
  });

  it("loads the next directory page on demand", async () => {
    getDirectoriesPageMock.mockResolvedValueOnce(
      directoryPage([rootEntries[0]], 101),
    );
    getDirectoriesPageMock.mockResolvedValueOnce(
      directoryPage([rootEntries[1]], 101, 100),
    );
    renderDialog();

    expect(await screen.findByText("图片")).toBeInTheDocument();
    expect(
      screen.getByText("第 1 / 2 页，共 101 个子文件夹"),
    ).toBeInTheDocument();
    await fireEvent.click(screen.getByRole("button", { name: "下一页" }));

    expect(await screen.findByText("文档")).toBeInTheDocument();
    expect(getDirectoriesPageMock).toHaveBeenLastCalledWith("", {
      limit: 100,
      offset: 100,
    });
  });

  it("blocks batches above the server move limit", async () => {
    renderDialog({
      items: Array.from({ length: 501 }, (_, index) => ({
        name: `file-${index}.txt`,
        path: `docs/file-${index}.txt`,
        kind: "file" as const,
      })),
    });

    expect(
      await screen.findByText("一次最多移动 500 个项目"),
    ).toBeInTheDocument();
    expect(
      screen.getByRole("button", { name: "移动到当前目录" }),
    ).toBeDisabled();
  });
});
