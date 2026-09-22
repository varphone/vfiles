import { fireEvent, render, screen, waitFor } from "@testing-library/vue";
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
  it("shows a skeleton while loading", () => {
    const { container } = renderModal({ preview: state({ loading: true }) });

    // 加载态统一使用骨架（结构内容用骨架而不是 spinner）
    const skeleton = container.querySelector('[role="status"]')!;
    expect(skeleton).not.toBeNull();
    expect(skeleton.getAttribute("aria-busy")).toBe("true");
    expect(skeleton.getAttribute("aria-label")).toContain("加载预览");
    expect(container.querySelectorAll(".skeleton-row").length).toBeGreaterThan(
      0,
    );
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

  it("warns for unsupported kinds and offers a download", async () => {
    const { emitted } = renderModal({
      preview: state({ kind: "unsupported" }),
      file: { name: "a.xyz", path: "a.xyz", kind: "file" },
    });

    expect(screen.getByText("暂不支持在线预览")).toBeInTheDocument();
    await fireEvent.click(screen.getByText("下载文件"));
    expect(emitted()["download"]).toHaveLength(1);
  });

  it("shows the copy button only when copying is possible", async () => {
    const { emitted, unmount } = renderModal({
      canCopy: true,
      copyState: "idle",
    });
    await fireEvent.click(screen.getByText("复制内容"));
    expect(emitted()["copy"]).toHaveLength(1);
    unmount();

    renderModal({ canCopy: false });
    expect(screen.queryByText("复制内容")).toBeNull();
  });

  it("reflects the copy feedback label", () => {
    const { unmount } = renderModal({ canCopy: true, copyState: "done" });
    expect(screen.getByText("已复制")).toBeInTheDocument();
    unmount();

    renderModal({ canCopy: true, copyState: "failed" });
    expect(screen.getByText("复制失败")).toBeInTheDocument();
  });

  it("offers 下载 and 所在文件夹 for the previewed file", async () => {
    const { emitted } = renderModal({
      file: { name: "main.ts", path: "src/main.ts", kind: "file" },
    });

    await fireEvent.click(screen.getByText("下载"));
    expect(emitted()["download"]).toHaveLength(1);

    await fireEvent.click(screen.getByText("所在文件夹"));
    expect(emitted()["reveal"]).toHaveLength(1);
  });

  it("zooms, rotates and fits an image with buttons and keyboard", async () => {
    const { container, emitted } = renderModal({
      preview: state({ kind: "image", objectUrl: "blob:preview" }),
      file: { name: "图.png", path: "图.png", kind: "file" },
    });

    // PageUp/PageDown = 上一张/下一张（r74 补充键，与 ← → 同通道 emit）。
    // 监听器随 shellRef 挂载（flush: post）—— 断言须候 tick（r65 时序教训同款）
    await waitFor(() => {
      fireEvent.keyDown(window, { key: "PageUp" });
      expect(emitted().prev).not.toBeUndefined();
    });
    await fireEvent.keyDown(window, { key: "PageDown" });
    expect(emitted().next).toHaveLength(1);

    const image = () => container.querySelector<HTMLElement>(".preview-image")!;
    expect(container.querySelector(".preview-zoom")?.textContent).toBe("100%");

    await fireEvent.click(screen.getByLabelText("放大"));
    expect(container.querySelector(".preview-zoom")?.textContent).toBe("125%");
    expect(image().style.transform).toContain("scale(1.25)");

    await fireEvent.click(screen.getByLabelText("缩小"));
    expect(container.querySelector(".preview-zoom")?.textContent).toBe("100%");

    await fireEvent.click(screen.getByLabelText("向左旋转"));
    expect(image().style.transform).toContain("rotate(270deg)");

    // 键盘：+ 放大、0 适应（同时复位旋转）、R 旋转
    const shell = container.querySelector<HTMLElement>(".preview-shell")!;
    await fireEvent.keyDown(shell, { key: "+" });
    expect(container.querySelector(".preview-zoom")?.textContent).toBe("125%");
    await fireEvent.keyDown(shell, { key: "0" });
    expect(container.querySelector(".preview-zoom")?.textContent).toBe("100%");
    expect(image().style.transform).toContain("rotate(0deg)");
    await fireEvent.keyDown(shell, { key: "r" });
    expect(image().style.transform).toContain("rotate(90deg)");
  });

  it("scales the image with the mouse wheel", async () => {
    const { container } = renderModal({
      preview: state({ kind: "image", objectUrl: "blob:preview" }),
    });

    await fireEvent.wheel(container.querySelector(".preview-image")!, {
      deltaY: -120,
    });
    expect(container.querySelector(".preview-zoom")?.textContent).toBe("125%");

    await fireEvent.wheel(container.querySelector(".preview-image")!, {
      deltaY: 120,
    });
    expect(container.querySelector(".preview-zoom")?.textContent).toBe("100%");
  });

  it("only shows the zoom bar for images", () => {
    const { unmount } = renderModal({
      preview: state({ kind: "text", text: "body" }),
    });
    expect(document.querySelector(".preview-zoom-bar")).toBeNull();
    unmount();

    renderModal({ preview: state({ kind: "image", objectUrl: "blob:x" }) });
    expect(document.querySelector(".preview-zoom-bar")).not.toBeNull();
  });

  it("only shows the floating arrows for multi-file previews", async () => {
    const { unmount } = renderModal({ total: 1 });
    expect(screen.queryByLabelText("下一张")).toBeNull();
    expect(screen.getByText("单张")).toBeInTheDocument();
    unmount();

    const multi = renderModal({
      total: 3,
      position: 2,
      canGoPrev: true,
      canGoNext: false,
    });
    expect(screen.getByText("2 / 3")).toBeInTheDocument();
    await fireEvent.click(screen.getByLabelText("上一张"));
    expect(multi.emitted()["prev"]).toHaveLength(1);
    expect(screen.getByLabelText("下一张")).toBeDisabled();
  });
});
