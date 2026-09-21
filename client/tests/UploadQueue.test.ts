import { fireEvent, render, screen } from "@testing-library/vue";
import { describe, expect, it } from "vitest";
import UploadQueue, {
  type UploadQueueItemView,
} from "../src/components/file-uploader/UploadQueue.vue";

function item(
  overrides: Partial<UploadQueueItemView> = {},
): UploadQueueItemView {
  return {
    id: 1,
    file: new File(["x"], "报告.txt"),
    message: "上传 报告.txt",
    editable: true,
    status: "queued",
    percent: null,
    ...overrides,
  };
}

function renderQueue(items: UploadQueueItemView[]) {
  return render(UploadQueue as any, { props: { items } });
}

describe("UploadQueue.vue", () => {
  it("renders a compact row with name, size and status", () => {
    const { container } = renderQueue([item()]);

    expect(screen.getByText("报告.txt")).toBeInTheDocument();
    expect(container.querySelector(".upload-queue-status")?.textContent).toBe(
      "排队中",
    );
    // 排队的条目可以编辑版本备注
    expect(screen.getByLabelText("报告.txt 的版本备注")).toBeInTheDocument();
    expect(screen.getByText("取消")).toBeInTheDocument();
  });

  it("shows the relative directory for folder uploads", () => {
    const { container } = renderQueue([
      item({
        file: new File(["x"], "a.txt"),
        relativePath: "项目/2026/a.txt",
      }),
    ]);

    expect(container.querySelector(".upload-queue-dir")?.textContent).toBe(
      "项目/2026",
    );
  });

  it("shows progress only while uploading", () => {
    const { container, unmount } = renderQueue([
      item({ status: "uploading", percent: 42, editable: false }),
    ]);
    expect(container.querySelector(".upload-queue-status")?.textContent).toBe(
      "上传中 42%",
    );
    expect(container.querySelector(".progress")).not.toBeNull();
    // 上传中不显示备注输入
    expect(container.querySelector(".upload-queue-message")).toBeNull();
    unmount();

    const done = renderQueue([item({ status: "done", percent: 100 })]);
    expect(
      done.container.querySelector(".upload-queue-status")?.textContent,
    ).toBe("已完成");
    // 完成的行不再显示进度条
    expect(done.container.querySelector(".progress")).toBeNull();
  });

  it("emits retry for failed rows and remove for finished rows", async () => {
    const { emitted, unmount } = renderQueue([item({ status: "error" })]);
    await fireEvent.click(screen.getByText("重试"));
    expect(emitted()["retry"]?.[0]).toEqual([1]);
    await fireEvent.click(screen.getByLabelText("从列表移除"));
    expect(emitted()["remove"]?.[0]).toEqual([1]);
    unmount();

    const canceled = renderQueue([item({ status: "canceled" })]);
    await fireEvent.click(screen.getByText("重试"));
    expect(canceled.emitted()["retry"]?.[0]).toEqual([1]);
  });

  it("shows the failure reason and emits message edits", async () => {
    renderQueue([
      item({ status: "error", error: "网络错误", editable: false }),
    ]);

    expect(screen.getByText("网络错误")).toBeInTheDocument();

    const editable = renderQueue([item()]);
    await fireEvent.update(
      screen.getByLabelText("报告.txt 的版本备注"),
      "修正说明",
    );
    expect(editable.emitted()["update-message"]?.[0]).toEqual([1, "修正说明"]);
  });
});
