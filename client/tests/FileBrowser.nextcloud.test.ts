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
  it("renders the shared badge only for paths in the shared set", () => {
    const { container } = render(FileList as any, {
      props: {
        desktop: true,
        selectMode: false,
        selectedPaths: new Set<string>(),
        sharedPaths: new Set<string>(["a.txt"]),
        files: [
          file({ id: "a", name: "a.txt", path: "a.txt" }),
          file({ id: "b", name: "b.txt", path: "b.txt" }),
        ],
      },
    });

    const rows = container.querySelectorAll("tr.desktop-file-row");
    expect(rows).toHaveLength(2);
    const badgeA = rows[0].querySelector(".desktop-shared-badge");
    expect(badgeA).not.toBeNull();
    expect(badgeA!.textContent?.trim()).toBe("已共享");
    expect(rows[1].querySelector(".desktop-shared-badge")).toBeNull();
  });

  it("uses the Nextcloud column order with 共享 between 名称 and 大小", () => {
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
    expect(headers).toEqual(["", "名称", "共享", "大小", "修改时间", "操作"]);
    // 共享列不参与排序：没有排序按钮，aria-sort 为 none
    const sharedHeader = Array.from(
      container.querySelectorAll<HTMLElement>("thead th"),
    ).find((th) => th.textContent?.trim() === "共享")!;
    expect(within(sharedHeader).queryByRole("button")).toBeNull();
    expect(sharedHeader).toHaveAttribute("aria-sort", "none");
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
    expect(screen.getByText("main.ts")).toBeInTheDocument();
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
