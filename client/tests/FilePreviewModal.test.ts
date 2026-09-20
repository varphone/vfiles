import { fireEvent, render, screen } from "@testing-library/vue";
import { describe, expect, it } from "vitest";
import FilePreviewModal from "../src/components/file-browser/FilePreviewModal.vue";
import type { PreviewState } from "../src/composables/useFilePreview";

function state(overrides: Partial<PreviewState> = {}): PreviewState {
  return {
    open: true,
    loading: false,
    error: null,
    path: "src/main.ts",
    kind: "code",
    text: "",
    html: "",
    objectUrl: "",
    ...overrides,
  };
}

function renderModal(props: Record<string, unknown> = {}) {
  return render(FilePreviewModal as any, {
    props: {
      show: true,
      filename: "main.ts",
      preview: state(),
      canGoPrev: true,
      canGoNext: true,
      position: 1,
      total: 1,
      canCopy: false,
      copyState: "idle",
      ...props,
    },
  });
}

describe("FilePreviewModal.vue", () => {
  it("shows the loading state", () => {
    renderModal({ preview: state({ loading: true }) });

    expect(screen.getByText("加载预览中...")).toBeInTheDocument();
  });

  it("shows the error with a retry action that reports the path", async () => {
    const { emitted } = renderModal({
      preview: state({ error: "获取文件内容失败" }),
    });

    expect(screen.getByText("获取文件内容失败")).toBeInTheDocument();
    await fireEvent.click(screen.getByText("重试"));
    expect(emitted()["retry"]?.[0]).toEqual(["src/main.ts"]);
  });

  it("renders images, markdown and plain text by kind", () => {
    const { unmount } = renderModal({
      preview: state({ kind: "image", objectUrl: "blob:preview" }),
    });
    expect(screen.getByRole("img")).toHaveAttribute("src", "blob:preview");
    unmount();

    renderModal({
      preview: state({ kind: "text", text: "plain body" }),
    });
    expect(screen.getByText("plain body")).toBeInTheDocument();
  });

  it("renders highlighted code for code previews", () => {
    const { container } = renderModal({
      preview: state({
        kind: "code",
        html: '<span class="hljs-keyword">const</span> a = 1;',
      }),
    });

    const code = container.querySelector(".preview-code code");
    expect(code?.innerHTML).toContain("hljs-keyword");
  });

  it("warns for unsupported kinds", () => {
    renderModal({ preview: state({ kind: "unsupported" }) });

    expect(
      screen.getByText("暂不支持该文件类型的在线预览，请使用下载。"),
    ).toBeInTheDocument();
  });

  it("shows the copy button only when copying is possible", async () => {
    const { emitted, unmount } = renderModal({
      canCopy: true,
      copyState: "idle",
    });
    await fireEvent.click(screen.getByText("复制"));
    expect(emitted()["copy"]).toHaveLength(1);
    unmount();

    renderModal({ canCopy: false });
    expect(screen.queryByText("复制")).toBeNull();
  });

  it("reflects the copy feedback label", () => {
    const { unmount } = renderModal({ canCopy: true, copyState: "done" });
    expect(screen.getByText("已复制")).toBeInTheDocument();
    unmount();

    renderModal({ canCopy: true, copyState: "failed" });
    expect(screen.getByText("复制失败")).toBeInTheDocument();
  });

  it("only shows navigation for multi-file previews", async () => {
    const { unmount } = renderModal({ total: 1 });
    expect(screen.queryByText("下一张")).toBeNull();
    unmount();

    const multi = renderModal({
      total: 3,
      position: 2,
      canGoPrev: true,
      canGoNext: false,
    });
    expect(screen.getByText("2 / 3")).toBeInTheDocument();
    await fireEvent.click(screen.getByText("上一张"));
    expect(multi.emitted()["prev"]).toHaveLength(1);
    // 已到末尾时「下一张」禁用
    expect(
      (screen.getByText("下一张").closest("button") as HTMLButtonElement)
        .disabled,
    ).toBe(true);
  });
});
