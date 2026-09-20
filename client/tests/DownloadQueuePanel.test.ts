import { render, screen, fireEvent } from "@testing-library/vue";
import { describe, expect, it } from "vitest";
import DownloadQueuePanel from "../src/components/file-browser/DownloadQueuePanel.vue";
import type { DownloadQueueItem } from "../src/composables/useDownloadQueue";

function item(overrides: Partial<DownloadQueueItem> = {}): DownloadQueueItem {
  return {
    id: 1,
    kind: "file",
    path: "docs/a.txt",
    filename: "a.txt",
    status: "queued",
    ...overrides,
  };
}

describe("DownloadQueuePanel.vue", () => {
  it("renders each queued item with its status", () => {
    render(DownloadQueuePanel as any, {
      props: {
        items: [
          item({ id: 1, filename: "a.txt", status: "queued" }),
          item({
            id: 2,
            filename: "photos.zip",
            kind: "folder",
            status: "downloading",
            progress: { loaded: 512, total: 1024 },
          }),
          item({
            id: 3,
            filename: "b.txt",
            status: "error",
            error: "网络错误",
          }),
        ],
      },
    });

    expect(screen.getByText("a.txt")).toBeInTheDocument();
    expect(screen.getByText("排队中")).toBeInTheDocument();
    expect(screen.getByText("ZIP")).toBeInTheDocument();
    expect(screen.getByText("网络错误")).toBeInTheDocument();
    expect(screen.getByText(/50%/)).toBeInTheDocument();
  });

  it("emits panel actions", async () => {
    const { emitted } = render(DownloadQueuePanel as any, {
      props: { items: [item({ id: 7 })], collapsed: false },
    });

    await fireEvent.click(screen.getByRole("button", { name: "最小化" }));
    expect(emitted()["toggle"]).toHaveLength(1);

    await fireEvent.click(screen.getByRole("button", { name: "清空已完成" }));
    expect(emitted()["clear-finished"]).toHaveLength(1);

    await fireEvent.click(screen.getByRole("button", { name: "全部取消" }));
    expect(emitted()["cancel-all"]).toHaveLength(1);

    await fireEvent.click(screen.getByRole("button", { name: "取消" }));
    expect(emitted()["cancel"]?.[0]).toEqual([7]);
  });

  it("emits remove for finished items", async () => {
    const { emitted } = render(DownloadQueuePanel as any, {
      props: { items: [item({ id: 9, status: "done" })], collapsed: false },
    });

    await fireEvent.click(screen.getByRole("button", { name: "移除" }));
    expect(emitted()["remove"]?.[0]).toEqual([9]);
  });

  it("hides the item list when collapsed", () => {
    render(DownloadQueuePanel as any, {
      props: { items: [item({ filename: "hidden.txt" })], collapsed: true },
    });

    expect(screen.queryByText("hidden.txt")).not.toBeInTheDocument();
    expect(screen.getByRole("button", { name: "展开" })).toBeInTheDocument();
  });

  it("renders nothing without items", () => {
    const { container } = render(DownloadQueuePanel as any, {
      props: { items: [] },
    });
    expect(container.querySelector(".box")).toBeNull();
  });

  it("offers retry for failed or canceled items", async () => {
    const { emitted } = render(DownloadQueuePanel as any, {
      props: {
        items: [
          item({ id: 3, status: "error", error: "网络错误" }),
          item({ id: 4, status: "canceled" }),
        ],
        collapsed: false,
      },
    });

    const retryButtons = screen.getAllByRole("button", { name: "重试" });
    expect(retryButtons).toHaveLength(2);

    await fireEvent.click(retryButtons[0]);
    expect(emitted()["retry"]?.[0]).toEqual([3]);
  });

  it("does not offer retry for finished items", () => {
    render(DownloadQueuePanel as any, {
      props: { items: [item({ id: 5, status: "done" })], collapsed: false },
    });

    expect(
      screen.queryByRole("button", { name: "重试" }),
    ).not.toBeInTheDocument();
    expect(screen.getByRole("button", { name: "移除" })).toBeInTheDocument();
  });
});
