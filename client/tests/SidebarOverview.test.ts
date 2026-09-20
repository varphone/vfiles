import { render, screen, waitFor } from "@testing-library/vue";
import { beforeEach, describe, expect, it, vi } from "vitest";
import SidebarOverview from "../src/components/file-browser/SidebarOverview.vue";

const { getOverviewMock } = vi.hoisted(() => ({ getOverviewMock: vi.fn() }));

vi.mock("../src/services/files.service", () => ({
  SEARCH_PAGE_SIZE: 100,
  filesService: { getOverview: getOverviewMock },
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

  it("keeps failures local instead of throwing", async () => {
    getOverviewMock.mockRejectedValueOnce(new Error("加载概览失败"));

    render(SidebarOverview as any);

    expect(await screen.findByText(/加载概览失败/)).toBeInTheDocument();
    expect(screen.getByText("重试")).toBeInTheDocument();
  });
});
