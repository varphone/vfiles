import { fireEvent, render, screen, within } from "@testing-library/vue";
import { describe, expect, it, vi } from "vitest";
import FileCard from "../src/components/file-browser/FileCard.vue";
import type { FileInfo } from "../src/types";

vi.mock("../src/services/files.service", () => ({
  SEARCH_PAGE_SIZE: 100,
  filesService: {
    getFilesPage: vi.fn(),
    thumbnailUrl: () => "/thumb",
  },
}));

function file(overrides: Partial<FileInfo> = {}): FileInfo {
  return {
    id: "readme.md",
    name: "readme.md",
    path: "readme.md",
    kind: "file",
    size_bytes: 2048,
    created_at: "2026-04-10T00:00:00.000Z",
    updated_at: "2026-04-10T00:00:00.000Z",
    ...overrides,
  };
}

describe("FileCard.vue action menu", () => {
  it("keeps the menu outside the thumbnail so it cannot be clipped", () => {
    const { container } = render(FileCard as any, { props: { file: file() } });

    const menu = container.querySelector(".file-card-menu")!;
    const thumb = container.querySelector(".file-card-thumb")!;

    // 缩略图容器带 overflow: hidden（裁剪圆角），菜单若在其内部会被裁掉
    expect(thumb.contains(menu)).toBe(false);
    expect(menu.parentElement?.classList.contains("file-card")).toBe(true);
  });

  it("offers the full action set for a file", async () => {
    const { container } = render(FileCard as any, { props: { file: file() } });
    await fireEvent.click(container.querySelector(".file-card-menu-trigger")!);

    const panel = container.querySelector(".file-card-menu-panel")!;
    const labels = within(panel as HTMLElement)
      .getAllByRole("menuitem")
      .map((el) => el.textContent?.trim());

    expect(labels).toEqual([
      "预览",
      "历史版本",
      "重命名",
      "移动",
      "下载",
      "分享",
      "删除",
    ]);
  });

  it("offers folder-specific entries for a directory", async () => {
    const { container } = render(FileCard as any, {
      props: {
        file: file({
          id: "docs",
          name: "docs",
          path: "docs",
          kind: "directory",
        }),
      },
    });
    await fireEvent.click(container.querySelector(".file-card-menu-trigger")!);

    const panel = container.querySelector(".file-card-menu-panel")!;
    const labels = within(panel as HTMLElement)
      .getAllByRole("menuitem")
      .map((el) => el.textContent?.trim());

    expect(labels).toEqual([
      "在此新建子目录",
      "打开",
      "重命名",
      "移动",
      "下载",
      "分享",
      "删除",
    ]);
    expect(screen.queryByText("预览")).toBeNull();
  });
});
