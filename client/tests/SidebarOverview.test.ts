import { render, screen, waitFor, within } from "@testing-library/vue";
import { beforeEach, describe, expect, it, vi } from "vitest";
import SidebarOverview from "../src/components/file-browser/SidebarOverview.vue";

const { getOverviewMock, getFavoritesMock, removeFavoriteMock } = vi.hoisted(
  () => ({
    getOverviewMock: vi.fn(),
    getFavoritesMock: vi.fn(),
    removeFavoriteMock: vi.fn(),
  }),
);

vi.mock("../src/services/files.service", () => ({
  SEARCH_PAGE_SIZE: 100,
  filesService: {
    getOverview: getOverviewMock,
    getFavorites: getFavoritesMock,
    removeFavorite: removeFavoriteMock,
  },
}));

function overview(overrides: Record<string, unknown> = {}) {
  return {
    file_count: 3,
    directory_count: 2,
    total_bytes: 2048,
    recent_files: [
      {
        path: "docs/report.md",
        name: "report.md",
        size_bytes: 1024,
        mime_type: "text/markdown",
        updated_at: "2026-09-20T10:00:00.000Z",
      },
    ],
    ...overrides,
  };
}

describe("SidebarOverview.vue", () => {
  beforeEach(() => {
    getOverviewMock.mockReset();
    getOverviewMock.mockResolvedValue(overview());
    getFavoritesMock.mockReset();
    getFavoritesMock.mockResolvedValue([]);
    removeFavoriteMock.mockReset();
    removeFavoriteMock.mockResolvedValue([]);
  });

  it("renders storage usage and recent files", async () => {
    render(SidebarOverview as any);

    expect(await screen.findByText("2.0 KB")).toBeInTheDocument();
    expect(screen.getByText("3 文件 · 2 目录")).toBeInTheDocument();
    expect(screen.getByText("最近更新")).toBeInTheDocument();
    expect(screen.getByText("report.md")).toBeInTheDocument();
  });

  it("emits the clicked recent file", async () => {
    const { emitted } = render(SidebarOverview as any);
    const item = await screen.findByTitle("docs/report.md");

    item.click();

    const events = emitted()["open-file"] as unknown[][];
    expect(events[0][0]).toMatchObject({ path: "docs/report.md" });
  });

  it("reloads when the refresh key changes", async () => {
    const { rerender } = render(SidebarOverview as any, {
      props: { refreshKey: 0 },
    });
    await waitFor(() => expect(getOverviewMock).toHaveBeenCalledTimes(1));

    await rerender({ refreshKey: 1 });

    await waitFor(() => expect(getOverviewMock).toHaveBeenCalledTimes(2));
  });

  it("lists favorites and removes one from the sidebar", async () => {
    getFavoritesMock.mockResolvedValue([
      { path: "docs/report.md", name: "report.md", kind: "file" },
    ]);

    const { emitted } = render(SidebarOverview as any);
    expect(await screen.findByText("收藏")).toBeInTheDocument();
    // 收藏与「最近更新」都会出现 report.md，这里限定在收藏区块内断言
    const favoriteBlock = screen.getByText("收藏").closest("div")!;
    expect(
      within(favoriteBlock).getByTitle("docs/report.md"),
    ).toBeInTheDocument();

    removeFavoriteMock.mockResolvedValue([]);
    (screen.getByLabelText("取消收藏 report.md") as HTMLElement).click();

    await waitFor(() =>
      expect(removeFavoriteMock).toHaveBeenCalledWith("docs/report.md"),
    );
    await waitFor(() => expect(screen.queryByText("收藏")).toBeNull());
    const events = emitted()["favorites-changed"] as unknown[][];
    expect(events[events.length - 1]?.[0]).toEqual([]);
  });

  it("emits the clicked favorite", async () => {
    getFavoritesMock.mockResolvedValue([
      { path: "docs", name: "docs", kind: "directory" },
    ]);

    const { emitted } = render(SidebarOverview as any);
    const item = await screen.findByTitle("docs");
    item.click();

    const events = emitted()["open-favorite"] as unknown[][];
    expect(events[0][0]).toMatchObject({ path: "docs", kind: "directory" });
  });

  it("still shows usage when only the favorites request fails", async () => {
    // 老库缺少 favorites 表时接口会 500，但存储用量/最近更新不应因此消失
    getFavoritesMock.mockRejectedValue(new Error("服务器内部错误"));

    render(SidebarOverview as any);

    expect(await screen.findByText("2.0 KB")).toBeInTheDocument();
    expect(screen.getByText("report.md")).toBeInTheDocument();
    expect(screen.queryByText(/服务器内部错误/)).toBeNull();
    expect(screen.queryByText("收藏")).toBeNull();
  });

  it("keeps failures local instead of throwing", async () => {
    getOverviewMock.mockRejectedValueOnce(new Error("加载概览失败"));

    render(SidebarOverview as any);

    expect(await screen.findByText(/加载概览失败/)).toBeInTheDocument();
    expect(screen.getByText("重试")).toBeInTheDocument();
  });
});
