import { render, screen } from "@testing-library/vue";
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
