import { fireEvent, render, screen } from "@testing-library/vue";
import { describe, expect, it } from "vitest";
import FileDetailsContent from "../src/components/file-browser/FileDetailsContent.vue";
import type { FileInfo } from "../src/types";

function file(overrides: Partial<FileInfo>): FileInfo {
  return {
    id: overrides.name ?? "id",
    name: overrides.name ?? "file",
    path: overrides.path ?? overrides.name ?? "file",
    kind: "file",
    created_at: "2026-09-20T10:00:00.000Z",
    updated_at: "2026-09-20T11:00:00.000Z",
    ...overrides,
  };
}

describe("FileDetailsContent.vue", () => {
  it("renders the metadata shared by the panel and the dialog", () => {
    render(FileDetailsContent as any, {
      props: {
        file: file({ name: "main.ts", path: "src/main.ts", size_bytes: 2048 }),
      },
    });

    expect(screen.getByText("main.ts")).toBeInTheDocument();
    expect(screen.getByText("/src/main.ts")).toBeInTheDocument();
    expect(screen.getByText("代码文件")).toBeInTheDocument();
    expect(screen.getByText("2.0 KB")).toBeInTheDocument();
    expect(screen.getByText("修改时间")).toBeInTheDocument();
    expect(screen.getByText("创建时间")).toBeInTheDocument();
  });

  it("shows dashes for directories and skips empty commit rows", () => {
    render(FileDetailsContent as any, {
      props: {
        file: file({ name: "docs", path: "docs", kind: "directory" }),
      },
    });

    expect(screen.getByText("文件夹")).toBeInTheDocument();
    expect(screen.getByText("--")).toBeInTheDocument();
    expect(screen.queryByText("最近提交")).toBeNull();
  });

  it("shows an image preview when a thumbnail url is available", async () => {
    const { container } = render(FileDetailsContent as any, {
      props: {
        file: file({ name: "logo.png", path: "图片/logo.png" }),
        previewUrl: "/api/files/thumbnail?path=...",
      },
    });

    const image = container.querySelector(
      ".details-preview-image",
    ) as HTMLImageElement;
    expect(image).not.toBeNull();
    expect(image.getAttribute("src")).toContain("/api/files/thumbnail");

    // 缩略图加载失败时回退到类型图标
    await fireEvent.error(image);
    expect(container.querySelector(".details-preview-image")).toBeNull();
  });

  it("renders the close button only when asked and emits close", async () => {
    const { container, emitted } = render(FileDetailsContent as any, {
      props: { file: file({ name: "a.txt" }), showClose: true },
    });

    const close = container.querySelector(
      ".details-close",
    ) as HTMLButtonElement;
    expect(close).not.toBeNull();
    await fireEvent.click(close);
    expect(emitted("close")).toHaveLength(1);
  });

  it("shows the parent folder as the location", () => {
    const { unmount } = render(FileDetailsContent as any, {
      props: { file: file({ name: "main.ts", path: "src/lib/main.ts" }) },
    });
    expect(screen.getByText("/src/lib")).toBeInTheDocument();
    unmount();

    render(FileDetailsContent as any, {
      props: { file: file({ name: "root.txt", path: "root.txt" }) },
    });
    expect(screen.getByText("位置")).toBeInTheDocument();
    expect(screen.getAllByText("/").length).toBeGreaterThan(0);
  });

  it("only renders the actions slot when provided", () => {
    const { container, unmount } = render(FileDetailsContent as any, {
      props: { file: file({ name: "a.txt" }) },
    });
    expect(container.querySelector(".desktop-details-actions")).toBeNull();
    unmount();

    const withSlot = render(FileDetailsContent as any, {
      props: { file: file({ name: "a.txt" }) },
      slots: { actions: "<button>预览</button>" },
    });
    expect(
      withSlot.container.querySelector(".desktop-details-actions"),
    ).not.toBeNull();
    expect(screen.getByText("预览")).toBeInTheDocument();
  });
});
