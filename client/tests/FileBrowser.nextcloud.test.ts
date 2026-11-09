import {
  fireEvent,
  render,
  screen,
  waitFor,
  within,
} from "@testing-library/vue";
import { beforeEach, describe, expect, it, vi } from "vitest";
import FileList from "../src/components/file-browser/FileList.vue";
import FileDetailsPanel from "../src/components/file-browser/FileDetailsPanel.vue";
import NewMenuButton from "../src/components/file-browser/NewMenuButton.vue";
import type { FileInfo } from "../src/types";

const { getFileHistoryMock, listSharesPageMock } = vi.hoisted(() => ({
  getFileHistoryMock: vi.fn(),
  listSharesPageMock: vi.fn(),
}));

vi.mock("../src/services/files.service", () => ({
  SEARCH_PAGE_SIZE: 100,
  filesService: {
    thumbnailUrl: (path: string, opts?: { size?: number }) =>
      `/api/files/thumbnail?path=${encodeURIComponent(path)}&size=${opts?.size ?? 0}`,
    getFileHistory: getFileHistoryMock,
    listSharesPage: listSharesPageMock,
  },
}));

function file(overrides: Partial<FileInfo> = {}): FileInfo {
  return {
    id: overrides.path ?? "file",
    name: overrides.name ?? "a.txt",
    path: overrides.path ?? "a.txt",
    kind: "file",
    size_bytes: 1024,
    created_at: "2026-09-20T09:00:00.000Z",
    updated_at: "2026-09-20T10:00:00.000Z",
    ...overrides,
  };
}

beforeEach(() => {
  getFileHistoryMock.mockReset();
  listSharesPageMock.mockReset();
  getFileHistoryMock.mockResolvedValue({
    currentVersion: "abc123456789",
    totalCommits: 0,
    nextCursor: null,
    commits: [
      {
        hash: "abc123456789",
        message: "更新说明",
        author: { name: "Alice", email: "alice@example.com" },
        date: "2026-09-20T10:00:00.000Z",
        parent: [],
      },
    ],
  });
  listSharesPageMock.mockResolvedValue({
    items: [],
    total: 0,
    limit: 100,
    offset: 0,
    has_more: false,
  });
});

describe("FileList.vue Nextcloud columns", () => {
  it("renders the inline Shared marker in the name column", () => {
    const { container } = render(FileList as any, {
      props: {
        desktop: true,
        selectMode: false,
        selectedPaths: new Set<string>(),
        sharedPaths: new Set<string>(["a.txt", "b.txt"]),
        publicSharedPaths: new Set<string>(["b.txt"]),
        files: [
          file({ id: "a", name: "a.txt", path: "a.txt" }),
          file({ id: "b", name: "b.txt", path: "b.txt" }),
          file({ id: "c", name: "c.txt", path: "c.txt" }),
        ],
      },
    });

    const rows = container.querySelectorAll("tr.desktop-file-row");
    expect(rows).toHaveLength(3);

    // 已共享标记在名称列内、文件名右侧：灰色小字 + 人物/链接图标
    const nameCell = rows[0].querySelector(".desktop-name-cell")!;
    const marker = nameCell.querySelector(".desktop-shared-inline");
    expect(marker).not.toBeNull();
    expect(marker!.textContent?.trim()).toBe("Shared");
    expect(marker!.querySelector("svg")).not.toBeNull();
    // 内部分享用人物图标，公开链接分享用链接图标
    expect(marker!.querySelector(".tabler-icon-users")).not.toBeNull();
    const linkMarker = rows[1].querySelector(
      ".desktop-name-cell .desktop-shared-inline",
    )!;
    expect(linkMarker.querySelector(".tabler-icon-link")).not.toBeNull();
    expect(linkMarker.querySelector(".tabler-icon-users")).toBeNull();
    // 未分享：名称列里没有标记；旧的独立「共享」列/徽标已删除
    expect(rows[2].querySelector(".desktop-shared-inline")).toBeNull();
    expect(container.querySelector(".desktop-shared-badge")).toBeNull();
    expect(container.querySelector(".desktop-file-shared")).toBeNull();
  });

  it("uses the Nextcloud column order without a separate 共享 column", () => {
    const { container } = render(FileList as any, {
      props: {
        desktop: true,
        selectMode: false,
        selectedPaths: new Set<string>(),
        files: [file()],
      },
    });

    const headers = Array.from(container.querySelectorAll("thead th")).map(
      (th) => th.textContent?.trim(),
    );
    expect(headers).toEqual(["", "名称", "大小", "修改时间", ""]);
    expect(headers).not.toContain("共享");
    // 行尾操作列：空表头、不参与排序（⋯ 与行操作图标在行内）
    const rowEndHeader = container.querySelectorAll<HTMLElement>("thead th")
      .item(4);
    expect(within(rowEndHeader).queryByRole("button")).toBeNull();
  });
});

describe("FileDetailsPanel.vue tabs", () => {
  it("switches between details and activity without losing the item", async () => {
    render(FileDetailsPanel as any, {
      props: { item: file({ name: "main.ts", path: "docs/main.ts" }) },
    });

    expect(screen.getByRole("tab", { name: "详细信息" })).toHaveAttribute(
      "aria-selected",
      "true",
    );
    expect(screen.getByText("/docs/main.ts")).toBeInTheDocument();

    await fireEvent.click(screen.getByRole("tab", { name: "活动" }));

    await waitFor(() => expect(getFileHistoryMock).toHaveBeenCalledTimes(1));
    expect(getFileHistoryMock).toHaveBeenCalledWith(
      "docs/main.ts",
      20,
      undefined,
      expect.objectContaining({ withTotal: false }),
    );
    expect(await screen.findByText("更新说明")).toBeInTheDocument();
    expect(screen.getByText("Alice")).toBeInTheDocument();

    await fireEvent.click(screen.getByRole("tab", { name: "详细信息" }));
    // 切回详情：仍是同一个条目，选中态没有被 Tab 切换清掉
    expect(screen.getByText("/docs/main.ts")).toBeInTheDocument();
    expect(screen.getAllByText("main.ts").length).toBeGreaterThan(0);
  });

  it("offers four icon tabs in the Nextcloud order and switches between them", async () => {
    const { container } = render(FileDetailsPanel as any, {
      props: { item: file({ name: "main.ts", path: "docs/main.ts" }) },
    });

    const tabs = screen.getAllByRole("tab");
    expect(tabs.map((tab) => tab.textContent?.trim())).toEqual([
      "详细信息",
      "分享",
      "活动",
      "版本",
    ]);
    // 图标在上、文字在下：每个 Tab 都带图标
    expect(container.querySelectorAll(".details-tab")).toHaveLength(4);
    expect(
      container.querySelectorAll(".details-tab .details-tab-icon"),
    ).toHaveLength(4);
    expect(tabs[0]).toHaveAttribute("aria-selected", "true");
    expect(tabs[0]).toHaveClass("is-active");

    await fireEvent.click(screen.getByRole("tab", { name: "分享" }));
    expect(screen.getByRole("tab", { name: "分享" })).toHaveAttribute(
      "aria-selected",
      "true",
    );
    expect(screen.getByRole("tab", { name: "分享" })).toHaveClass("is-active");
    expect(tabs[0]).toHaveAttribute("aria-selected", "false");
    // 分享 Tab：内部分享 / 外部分享两个区块
    expect(screen.getByText("内部分享")).toBeInTheDocument();
    expect(screen.getByText("外部分享")).toBeInTheDocument();
  });

  it("lists only this entry's share links and opens the share dialog", async () => {
    listSharesPageMock.mockResolvedValue({
      items: [
        {
          id: "share-1",
          entry_id: "main",
          entry_name: "main.ts",
          entry_path: "/docs/main.ts",
          entry_kind: "file",
          code: "abc12345",
          expires_at: null,
          created_at: "2026-09-20T10:00:00.000Z",
          access_count: 2,
          last_accessed_at: null,
        },
        {
          id: "share-2",
          entry_id: "other",
          entry_name: "other.txt",
          entry_path: "docs/other.txt",
          entry_kind: "file",
          code: "other999",
          expires_at: null,
          created_at: "2026-09-20T10:00:00.000Z",
          access_count: 0,
          last_accessed_at: null,
        },
      ],
      total: 2,
      limit: 100,
      offset: 0,
      has_more: false,
    });

    const { emitted } = render(FileDetailsPanel as any, {
      props: { item: file({ name: "main.ts", path: "docs/main.ts" }) },
    });

    await fireEvent.click(screen.getByRole("tab", { name: "分享" }));

    expect(await screen.findByText(/\/s\/abc12345/)).toBeInTheDocument();
    expect(screen.queryByText(/\/s\/other999/)).toBeNull();

    await fireEvent.click(screen.getByText("创建分享"));
    const shareEvents = emitted("share") as unknown as [[{ path: string }]];
    expect(shareEvents).toHaveLength(1);
    expect(shareEvents[0][0].path).toBe("docs/main.ts");
  });

  it("hides share creation for a read-only session", async () => {
    render(FileDetailsPanel as any, {
      props: {
        item: file({ name: "main.ts", path: "docs/main.ts" }),
        canShare: false,
        canWrite: false,
      },
    });

    await fireEvent.click(screen.getByRole("tab", { name: "分享" }));

    expect(screen.queryByText("创建分享")).toBeNull();
    expect(
      await screen.findByText("当前会话没有分享权限。"),
    ).toBeInTheDocument();
  });
});

describe("NewMenuButton.vue", () => {
  it("opens the Nextcloud + new menu and emits the chosen action", async () => {
    const { emitted } = render(NewMenuButton as any);

    expect(screen.queryByRole("menuitem", { name: "上传文件" })).toBeNull();

    await fireEvent.click(screen.getByRole("button", { name: "新建" }));
    expect(
      screen.getByRole("menuitem", { name: "上传文件" }),
    ).toBeInTheDocument();
    expect(
      screen.getByRole("menuitem", { name: "上传文件夹" }),
    ).toBeInTheDocument();
    expect(
      screen.getByRole("menuitem", { name: "新建文件夹" }),
    ).toBeInTheDocument();

    await fireEvent.click(screen.getByRole("menuitem", { name: "上传文件夹" }));
    expect(emitted("upload-folder")).toHaveLength(1);
    // 选择后菜单收起
    expect(screen.queryByRole("menuitem", { name: "上传文件" })).toBeNull();

    await fireEvent.click(screen.getByRole("button", { name: "新建" }));
    await fireEvent.click(screen.getByRole("menuitem", { name: "新建文件夹" }));
    expect(emitted("create-folder")).toHaveLength(1);
  });

  it("closes the menu with Escape", async () => {
    render(NewMenuButton as any);

    await fireEvent.click(screen.getByRole("button", { name: "新建" }));
    expect(
      screen.getByRole("menuitem", { name: "上传文件" }),
    ).toBeInTheDocument();

    await fireEvent.keyDown(document, { key: "Escape" });
    expect(screen.queryByRole("menuitem", { name: "上传文件" })).toBeNull();
  });
});
