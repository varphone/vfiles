import { fireEvent, render, screen, waitFor } from "@testing-library/vue";
import { beforeEach, describe, expect, it, vi } from "vitest";
import BreadcrumbTreeMenu from "../src/components/file-browser/BreadcrumbTreeMenu.vue";
import type { FileInfo } from "../src/types";

const { getFilesPageMock } = vi.hoisted(() => ({ getFilesPageMock: vi.fn() }));

vi.mock("../src/services/files.service", () => ({
  SEARCH_PAGE_SIZE: 100,
  filesService: { getFilesPage: getFilesPageMock },
}));

function directory(path: string): FileInfo {
  return {
    id: path,
    name: path.split("/").pop() ?? path,
    path,
    kind: "directory",
    created_at: "2026-01-01T00:00:00.000Z",
  };
}

describe("BreadcrumbTreeMenu.vue", () => {
  beforeEach(() => {
    getFilesPageMock.mockReset();
    getFilesPageMock.mockResolvedValue({
      items: [],
      total: 0,
      limit: 200,
      offset: 0,
      has_more: false,
    });
  });

  it("shows the current level's subfolders and expands them lazily", async () => {
    getFilesPageMock.mockImplementation(async (path: string) => {
      if (path === "docs") {
        return {
          items: [directory("docs/api")],
          total: 1,
          limit: 200,
          offset: 0,
          has_more: false,
        };
      }
      return { items: [], total: 0, limit: 200, offset: 0, has_more: false };
    });

    const { emitted } = render(BreadcrumbTreeMenu as any, {
      props: { currentPath: "", directories: [directory("docs")] },
    });

    // 第一层来自父组件（onMounted 写入后需等一次渲染），不额外请求
    await waitFor(() => expect(screen.getByText("docs")).toBeInTheDocument());
    expect(getFilesPageMock).not.toHaveBeenCalled();

    // 展开后按需加载下一层
    await fireEvent.click(screen.getByLabelText("展开 docs"));

    await waitFor(() => expect(screen.getByText("api")).toBeInTheDocument());
    expect(getFilesPageMock).toHaveBeenCalledWith("docs", {
      limit: 200,
      offset: 0,
    });

    // 点击目录上报导航路径
    await fireEvent.click(screen.getByText("api"));
    const events = emitted()["navigate"] as unknown[][];
    expect(events[0][0]).toBe("docs/api");
  });

  it("tells the user when the current directory has no subfolders", async () => {
    render(BreadcrumbTreeMenu as any, {
      props: { currentPath: "", directories: [] },
    });

    await waitFor(() =>
      expect(screen.getByText("当前目录没有子文件夹")).toBeInTheDocument(),
    );
  });
});
