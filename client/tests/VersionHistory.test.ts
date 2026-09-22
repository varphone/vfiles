import { fireEvent, render, waitFor, within } from "@testing-library/vue";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { createPinia } from "pinia";
import VersionHistory from "../src/components/version-history/VersionHistory.vue";

const { historyMock, contentMock, diffMock, restoreMock } = vi.hoisted(() => ({
  historyMock: vi.fn(),
  contentMock: vi.fn(),
  diffMock: vi.fn(),
  restoreMock: vi.fn(),
}));

vi.mock("../src/services/files.service", () => ({
  SEARCH_PAGE_SIZE: 100,
  filesService: {
    getFileHistory: historyMock,
    getFileContent: contentMock,
    getFileDiff: diffMock,
    restoreFileVersion: restoreMock,
    downloadFile: vi.fn(),
  },
}));

vi.mock("../src/composables/dialog", () => ({
  confirmDialog: vi.fn(async () => true),
}));

const CURRENT = "1196ff62-51e2-4f07-bdcd-6431dd268ba6";
const PREVIOUS = "2fbc7ce8-9e27-439f-9778-1417d487e99b";

function history() {
  return {
    currentVersion: CURRENT,
    totalCommits: 6,
    commits: [
      {
        hash: CURRENT,
        message: "Restore version 1",
        changeType: "modified" as const,
        hasCustomMessage: true,
        author: { name: "smoke", email: "" },
        date: new Date().toISOString(),
        parent: [PREVIOUS],
      },
      {
        hash: PREVIOUS,
        message: "更新第 5 版，补充了说明文字",
        changeType: "modified" as const,
        hasCustomMessage: true,
        author: { name: "smoke", email: "" },
        date: new Date(Date.now() - 3600_000).toISOString(),
        parent: [],
      },
    ],
  };
}

function renderHistory() {
  return render(VersionHistory as any, {
    props: { filePath: "notes.txt" },
    global: { plugins: [createPinia()] },
  });
}

describe("VersionHistory.vue", () => {
  beforeEach(() => {
    historyMock.mockReset();
    historyMock.mockResolvedValue(history());
    contentMock.mockReset();
    contentMock.mockResolvedValue(
      new Blob(["line 1\nline 2\n"], { type: "text/plain" }),
    );
    diffMock.mockReset();
    diffMock.mockResolvedValue("--- a\n+++ b\n@@ -1 +1 @@\n-line 0\n+line 1");
    restoreMock.mockReset();
    restoreMock.mockResolvedValue({ success: true });
  });

  it("summarises the version count and current version", async () => {
    const { container } = renderHistory();

    await waitFor(() =>
      expect(container.querySelector(".history-summary")).not.toBeNull(),
    );
    const summary =
      container.querySelector(".history-summary")?.textContent ?? "";
    expect(summary).toContain("6");
    expect(summary).toContain("个版本");
    expect(summary).toContain("1196ff62");
    expect(container.querySelector(".history-summary-hash")).toHaveAttribute(
      "title",
      CURRENT,
    );
  });

  it("explains the retention policy above the list", async () => {
    const { container } = renderHistory();

    await waitFor(() =>
      expect(container.querySelector(".history-retention")).not.toBeNull(),
    );
    expect(
      container.querySelector(".history-retention")?.textContent,
    ).toContain("历史版本会长期保留");
  });

  it("uses the shared empty and error states", async () => {
    historyMock.mockResolvedValueOnce({
      commits: [],
      currentVersion: "",
      totalCommits: 0,
    });
    const empty = renderHistory();
    await waitFor(() =>
      expect(
        empty.container.querySelector(".empty-state-title")?.textContent,
      ).toBe("暂无历史记录"),
    );
    empty.unmount();

    historyMock.mockRejectedValueOnce(new Error("服务器内部错误"));
    const failed = renderHistory();
    await waitFor(() =>
      expect(
        failed.container.querySelector(".empty-state.is-error")?.textContent,
      ).toContain("服务器内部错误"),
    );
    // 失败态提供重试
    expect(
      failed.container.querySelector(".empty-state-actions button"),
    ).not.toBeNull();
  });

  it("marks the current version and only offers restore for older ones", async () => {
    const { container } = renderHistory();
    await waitFor(() =>
      expect(container.querySelectorAll(".history-row").length).toBe(2),
    );

    const rows = container.querySelectorAll(".history-row");
    expect(rows[0].classList.contains("is-current")).toBe(true);
    expect(
      within(rows[0] as HTMLElement).getByText("当前版本"),
    ).toBeInTheDocument();
    // 当前版本没有「恢复」按钮（标题里的「从历史版本恢复」不算），改为提示文案
    expect(
      within(rows[0] as HTMLElement).queryByRole("button", { name: "恢复" }),
    ).toBeNull();
    expect(
      within(rows[0] as HTMLElement).getByText("这是当前版本"),
    ).toBeInTheDocument();

    // 旧版本带序号与恢复按钮
    expect(
      within(rows[1] as HTMLElement).getByText("第 5 版"),
    ).toBeInTheDocument();
    expect(
      within(rows[1] as HTMLElement).getByRole("button", { name: "恢复" }),
    ).toBeInTheDocument();
  });

  it("exposes labelled actions instead of icon-only buttons", async () => {
    const { container } = renderHistory();
    await waitFor(() =>
      expect(container.querySelectorAll(".history-row").length).toBe(2),
    );

    const firstRow = container.querySelector(".history-row") as HTMLElement;
    const labels = within(firstRow)
      .getAllByRole("button")
      .map((el) => el.textContent?.trim());

    expect(labels).toEqual(["预览", "对比", "下载"]);
  });

  it("switches the detail pane between preview and diff", async () => {
    const { container } = renderHistory();
    await waitFor(() =>
      expect(container.querySelectorAll(".history-row").length).toBe(2),
    );

    const secondRow = container.querySelectorAll(
      ".history-row",
    )[1] as HTMLElement;

    await fireEvent.click(
      within(secondRow).getByRole("button", { name: "对比" }),
    );
    await waitFor(() =>
      expect(container.querySelector(".diff-block")).not.toBeNull(),
    );
    // 结构化逐行渲染（unified diff 解析为 ± 色带行）
    expect(
      container.querySelector(
        ".diff-line.is-add, .diff-line.is-del, .diff-line.is-ctx",
      ),
    ).not.toBeNull();
    expect(
      container.querySelector(".history-detail-label")?.textContent,
    ).toContain("版本对比");

    // 再点预览：同一面板切换为预览，不应继续显示对比结果
    await fireEvent.click(
      within(secondRow).getByRole("button", { name: "预览" }),
    );
    await waitFor(() =>
      expect(container.querySelector(".diff-block")).toBeNull(),
    );
    expect(
      container.querySelector(".history-detail-label")?.textContent,
    ).toContain("版本预览");
  });

  it("asks for confirmation before restoring an older version", async () => {
    const { container } = renderHistory();
    await waitFor(() =>
      expect(container.querySelectorAll(".history-row").length).toBe(2),
    );
    const { confirmDialog } = await import("../src/composables/dialog");

    const secondRow = container.querySelectorAll(
      ".history-row",
    )[1] as HTMLElement;
    await fireEvent.click(
      within(secondRow).getByRole("button", { name: "恢复" }),
    );

    await waitFor(() =>
      expect(restoreMock).toHaveBeenCalledWith(
        "notes.txt",
        PREVIOUS,
        "恢复历史版本",
      ),
    );
    expect(confirmDialog).toHaveBeenCalled();
  });
});
