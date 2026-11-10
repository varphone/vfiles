import { fireEvent, render, screen, waitFor } from "@testing-library/vue";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { createPinia, setActivePinia } from "pinia";
import { createMemoryHistory, createRouter } from "vue-router";
import UnifiedSearch from "../src/components/shell/UnifiedSearch.vue";

const { searchFilesMock } = vi.hoisted(() => ({
  searchFilesMock: vi.fn(),
}));

vi.mock("../src/services/files.service", () => ({
  filesService: {
    searchFiles: searchFilesMock,
  },
}));

const Blank = { render: () => null };

function renderSearch(show = true) {
  const pinia = createPinia();
  setActivePinia(pinia);
  const router = createRouter({
    history: createMemoryHistory(),
    routes: [
      { path: "/", name: "home", component: Blank },
      { path: "/activity", name: "activity", component: Blank },
    ],
  });
  const onClose = vi.fn();

  return {
    ...render(UnifiedSearch as any, {
      props: { show, onClose },
      global: { plugins: [pinia, router] },
    }),
    router,
    onClose,
  };
}

beforeEach(() => {
  searchFilesMock.mockReset();
  searchFilesMock.mockResolvedValue({
    items: [
      {
        id: "file-1",
        name: "report.pdf",
        path: "docs/report.pdf",
        kind: "file",
        size_bytes: 2048,
        created_at: "2026-05-29T00:00:00.000Z",
      },
    ],
    hasMore: false,
    resultsMayBeIncomplete: false,
    limit: 20,
    offset: 0,
  });
});

describe("UnifiedSearch.vue", () => {
  it("debounces the query and renders the grouped results", async () => {
    renderSearch();

    const input = screen.getByPlaceholderText("搜索文件与文件夹…");
    await fireEvent.update(input, "report");

    // 300ms 防抖：输入后不立即请求
    expect(searchFilesMock).not.toHaveBeenCalled();

    await waitFor(() =>
      expect(searchFilesMock).toHaveBeenCalledWith("report", "name", {
        limit: 20,
      }),
    );
    expect(searchFilesMock).toHaveBeenCalledTimes(1);

    expect(await screen.findByText("文件")).toBeInTheDocument();
    expect(screen.getByText("report.pdf")).toBeInTheDocument();
    expect(screen.getByText("docs/report.pdf")).toBeInTheDocument();
  });

  it("closes and jumps to the result directory when a result is clicked", async () => {
    const { router, onClose } = renderSearch();

    await fireEvent.update(
      screen.getByPlaceholderText("搜索文件与文件夹…"),
      "report",
    );

    const result = await screen.findByRole("button", {
      name: /report\.pdf/,
    });
    await fireEvent.click(result);

    expect(onClose).toHaveBeenCalledTimes(1);
    await waitFor(() =>
      expect(router.currentRoute.value.query.path).toBe("docs"),
    );
    expect(router.currentRoute.value.path).toBe("/");
  });

  it("offers the full result list when the preview hits the cap", async () => {
    searchFilesMock.mockResolvedValue({
      items: Array.from({ length: 20 }, (_, index) => ({
        id: `file-${index}`,
        name: `report-${index}.pdf`,
        path: `docs/report-${index}.pdf`,
        kind: "file",
        size_bytes: 1024,
        created_at: "2026-05-29T00:00:00.000Z",
      })),
      hasMore: true,
      resultsMayBeIncomplete: false,
      limit: 20,
      offset: 0,
    });
    const { router, onClose } = renderSearch();

    await fireEvent.update(
      screen.getByPlaceholderText("搜索文件与文件夹…"),
      "report",
    );
    const all = await screen.findByRole("button", { name: "查看全部结果" });
    await fireEvent.click(all);

    expect(onClose).toHaveBeenCalledTimes(1);
    await waitFor(() =>
      expect(router.currentRoute.value.query.q).toBe("report"),
    );
    expect(router.currentRoute.value.path).toBe("/");
  });

  it("closes on Escape and clears its results", async () => {
    const { onClose } = renderSearch();

    await fireEvent.update(
      screen.getByPlaceholderText("搜索文件与文件夹…"),
      "report",
    );
    await screen.findByText("report.pdf");

    await fireEvent.keyDown(document, { key: "Escape" });

    expect(onClose).toHaveBeenCalledTimes(1);
  });

  it("does not search an empty query", async () => {
    renderSearch();

    await fireEvent.update(
      screen.getByPlaceholderText("搜索文件与文件夹…"),
      "   ",
    );

    // 空查询直接清空结果，不触发请求
    await new Promise((resolve) => setTimeout(resolve, 350));
    expect(searchFilesMock).not.toHaveBeenCalled();
  });
});
