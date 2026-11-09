import { fireEvent, render, screen } from "@testing-library/vue";
import { describe, expect, it } from "vitest";
import LibraryGrid from "../src/components/file-browser/LibraryGrid.vue";
import type { FileInfo } from "../src/types";

function buildDirectory(overrides: Partial<FileInfo> = {}): FileInfo {
  return {
    id: "dir-id",
    name: "目录",
    path: "目录",
    kind: "directory",
    created_at: "2026-05-29T00:00:00.000Z",
    updated_at: "2026-05-29T00:00:00.000Z",
    ...overrides,
  };
}

function renderGrid(directories: FileInfo[]) {
  return render(LibraryGrid as any, {
    props: { directories },
  });
}

describe("LibraryGrid.vue", () => {
  it("renders one Seafile-style card per root directory", () => {
    const { container } = renderGrid([
      buildDirectory({ id: "docs", name: "文档", path: "docs" }),
      buildDirectory({ id: "design", name: "设计稿", path: "design" }),
    ]);

    expect(screen.getByText("我的资料库")).toBeInTheDocument();
    expect(screen.getByText("2 个目录")).toBeInTheDocument();
    expect(container.querySelectorAll(".library-card")).toHaveLength(2);
    expect(screen.getByText("文档")).toBeInTheDocument();
    expect(screen.getByText("设计稿")).toBeInTheDocument();
    expect(screen.getAllByText("文件夹")).toHaveLength(2);
  });

  it("enters a directory when its card is clicked", async () => {
    const { emitted } = renderGrid([
      buildDirectory({ id: "docs", name: "文档", path: "docs" }),
      buildDirectory({ id: "design", name: "设计稿", path: "design" }),
    ]);

    await fireEvent.click(screen.getByText("设计稿"));

    const opens = emitted()["open"] as Array<[FileInfo]>;
    expect(opens).toHaveLength(1);
    expect(opens[0][0].path).toBe("design");
  });

  it("exposes rename from the card overflow menu", async () => {
    const { emitted } = renderGrid([
      buildDirectory({ id: "docs", name: "文档", path: "docs" }),
      buildDirectory({ id: "design", name: "设计稿", path: "design" }),
    ]);

    await fireEvent.click(screen.getByLabelText("设计稿 的操作"));
    await fireEvent.click(screen.getByText("重命名"));

    const renames = emitted()["rename"] as Array<[FileInfo]>;
    expect(renames).toHaveLength(1);
    expect(renames[0][0].path).toBe("design");
  });

  it("shows the empty state when the root has no directories", () => {
    const { container } = renderGrid([]);

    expect(screen.getByText("还没有目录")).toBeInTheDocument();
    expect(screen.getByText("0 个目录")).toBeInTheDocument();
    expect(container.querySelectorAll(".library-card")).toHaveLength(0);
  });
});
