import { fireEvent, render, screen, waitFor } from "@testing-library/vue";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { createPinia, setActivePinia } from "pinia";
import { createRouter, createMemoryHistory } from "vue-router";
import SharedLinks from "../src/views/SharedLinks.vue";
import type { ShareLink } from "../src/types";

const { listSharesMock, disableShareMock, copyTextMock, confirmMock } =
  vi.hoisted(() => ({
    listSharesMock: vi.fn(),
    disableShareMock: vi.fn(async () => undefined),
    copyTextMock: vi.fn(async () => true),
    confirmMock: vi.fn(async () => true),
  }));

vi.mock("../src/services/files.service", () => ({
  SEARCH_PAGE_SIZE: 100,
  filesService: {
    listShares: listSharesMock,
    disableShare: disableShareMock,
  },
}));

vi.mock("../src/utils/clipboard", () => ({ copyText: copyTextMock }));
vi.mock("../src/composables/dialog", () => ({
  confirmDialog: confirmMock,
  promptDialog: vi.fn(async () => null),
}));

function share(overrides: Partial<ShareLink> = {}): ShareLink {
  return {
    id: "s1",
    entry_id: "e1",
    entry_name: "报告.txt",
    entry_path: "文档/报告.txt",
    entry_kind: "file",
    code: "abc123",
    expires_at: null,
    created_at: "2026-09-20T10:00:00.000Z",
    access_count: 3,
    last_accessed_at: null,
    ...overrides,
  };
}

function renderPage() {
  const pinia = createPinia();
  setActivePinia(pinia);
  const router = createRouter({ history: createMemoryHistory(), routes: [] });
  return render(SharedLinks, { global: { plugins: [pinia, router] } });
}

describe("SharedLinks.vue", () => {
  beforeEach(() => {
    listSharesMock.mockReset();
    listSharesMock.mockResolvedValue([share()]);
    disableShareMock.mockClear();
    copyTextMock.mockClear();
    confirmMock.mockClear();
  });

  it("lists shares with name, path, status and access count", async () => {
    const { container } = renderPage();

    await screen.findByText("报告.txt");
    expect(screen.getByText("/文档/报告.txt")).toBeInTheDocument();
    expect(screen.getByText("长期有效")).toBeInTheDocument();
    expect(screen.getByText("访问 3 次")).toBeInTheDocument();
    expect(container.querySelector(".shares-link-input")).toHaveValue(
      `${window.location.origin}/s/abc123`,
    );
    expect(container.querySelector(".shares-icon svg")).not.toBeNull();
  });

  it("copies and opens a link", async () => {
    const openSpy = vi.spyOn(window, "open").mockReturnValue(null);
    renderPage();

    await screen.findByText("报告.txt");
    await fireEvent.click(screen.getByText("复制"));
    await waitFor(() =>
      expect(copyTextMock).toHaveBeenCalledWith(
        `${window.location.origin}/s/abc123`,
      ),
    );
    expect(await screen.findByText("已复制")).toBeInTheDocument();

    await fireEvent.click(screen.getByText("打开"));
    expect(openSpy).toHaveBeenCalled();
    openSpy.mockRestore();
  });

  it("stops sharing after confirmation and removes the row", async () => {
    renderPage();
    await screen.findByText("报告.txt");

    await fireEvent.click(screen.getByText("停止"));

    await waitFor(() =>
      expect(disableShareMock).toHaveBeenCalledWith("abc123"),
    );
    expect(confirmMock).toHaveBeenCalled();
    await waitFor(() =>
      expect(screen.getByText("还没有分享链接")).toBeInTheDocument(),
    );
  });

  it("marks expired shares and keeps them listed", async () => {
    listSharesMock.mockResolvedValue([
      share({
        code: "old",
        entry_name: "旧文件.txt",
        expires_at: "2020-01-01T00:00:00.000Z",
      }),
    ]);
    const { container } = renderPage();

    await screen.findByText("旧文件.txt");
    // 「已过期」同时出现在筛选 chip 与行徽章上，这里只断言行内徽章
    expect(
      container
        .querySelector(".shares-row.is-expired .shares-badge")
        ?.textContent?.trim(),
    ).toBe("已过期");
    // 有效数量为 0 时副标题会说明
    expect(screen.getByText(/有效 0 个/)).toBeInTheDocument();
  });

  it("filters by status with counted chips", async () => {
    listSharesMock.mockResolvedValue([
      share({ code: "a", entry_name: "有效文件.txt", access_count: 0 }),
      share({
        code: "b",
        entry_name: "过期文件.txt",
        access_count: 0,
        expires_at: "2020-01-01T00:00:00.000Z",
      }),
      share({ code: "c", entry_name: "被访问.txt", access_count: 5 }),
    ]);
    const { container } = renderPage();

    await screen.findByText("有效文件.txt");
    const chips = Array.from(container.querySelectorAll(".shares-filter")).map(
      (chip) => ({
        label: chip.querySelector("span")?.textContent?.trim(),
        count: chip.querySelector(".shares-filter-count")?.textContent?.trim(),
      }),
    );
    expect(chips).toEqual([
      { label: "全部", count: "3" },
      { label: "有效", count: "2" },
      { label: "已过期", count: "1" },
      { label: "已被访问", count: "1" },
    ]);

    // 只看已过期
    await fireEvent.click(
      Array.from(
        container.querySelectorAll<HTMLButtonElement>(".shares-filter"),
      ).find((chip) => chip.textContent?.includes("已过期"))!,
    );
    await waitFor(() =>
      expect(
        Array.from(container.querySelectorAll(".shares-name")).map((el) =>
          el.textContent?.trim(),
        ),
      ).toEqual(["过期文件.txt"]),
    );

    // 只看已被访问
    await fireEvent.click(
      Array.from(
        container.querySelectorAll<HTMLButtonElement>(".shares-filter"),
      ).find((chip) => chip.textContent?.includes("已被访问"))!,
    );
    await waitFor(() =>
      expect(
        Array.from(container.querySelectorAll(".shares-name")).map((el) =>
          el.textContent?.trim(),
        ),
      ).toEqual(["被访问.txt"]),
    );
  });

  it("shows a filter-specific empty state", async () => {
    listSharesMock.mockResolvedValue([share({ code: "a" })]);
    const { container } = renderPage();
    await screen.findByText("报告.txt");

    await fireEvent.click(
      Array.from(
        container.querySelectorAll<HTMLButtonElement>(".shares-filter"),
      ).find((chip) => chip.textContent?.includes("已过期"))!,
    );

    await waitFor(() =>
      expect(container.querySelector(".empty-state-title")?.textContent).toBe(
        "没有已过期的链接",
      ),
    );
    await fireEvent.click(screen.getByText("查看全部"));
    await waitFor(() =>
      expect(container.querySelector(".shares-name")?.textContent?.trim()).toBe(
        "报告.txt",
      ),
    );
  });

  it("stops every expired link from the bulk action", async () => {
    listSharesMock.mockResolvedValue([
      share({
        code: "old1",
        entry_name: "过期一.txt",
        expires_at: "2020-01-01T00:00:00.000Z",
      }),
      share({
        code: "old2",
        entry_name: "过期二.txt",
        expires_at: "2021-01-01T00:00:00.000Z",
      }),
      share({ code: "new1", entry_name: "有效.txt" }),
    ]);
    const { container } = renderPage();
    await screen.findByText("过期一.txt");

    const bulk = Array.from(
      container.querySelectorAll<HTMLButtonElement>(
        ".shares-header-actions button",
      ),
    ).find((button) => button.textContent?.includes("清理过期链接"));
    expect(bulk?.textContent).toContain("2");

    await fireEvent.click(bulk!);

    await waitFor(() => expect(disableShareMock).toHaveBeenCalledTimes(2));
    const stoppedCodes = disableShareMock.mock.calls
      .map((call: unknown[]) => String(call[0]))
      .sort();
    expect(stoppedCodes).toEqual(["old1", "old2"]);
    // 过期链接被移除，有效链接保留
    await waitFor(() =>
      expect(container.querySelectorAll(".shares-row")).toHaveLength(1),
    );
    expect(container.querySelector(".shares-name")?.textContent?.trim()).toBe(
      "有效.txt",
    );
    // 清理按钮随之消失
    expect(
      Array.from(
        container.querySelectorAll(".shares-header-actions button"),
      ).some((button) => button.textContent?.includes("清理过期链接")),
    ).toBe(false);
  });

  it("shows the empty and error states", async () => {
    listSharesMock.mockResolvedValue([]);
    const empty = renderPage();
    // 组件在加载与空态之间会重渲染，统一重新查询当前 DOM
    await waitFor(() =>
      expect(
        empty.container.querySelector(".empty-state-title")?.textContent,
      ).toBe("还没有分享链接"),
    );
    empty.unmount();

    listSharesMock.mockRejectedValue(new Error("服务器内部错误"));
    const failed = renderPage();
    await waitFor(() =>
      expect(
        failed.container.querySelector(".empty-state.is-error")?.textContent,
      ).toContain("服务器内部错误"),
    );
  });
});
