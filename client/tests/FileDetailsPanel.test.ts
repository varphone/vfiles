import { fireEvent, render, screen } from "@testing-library/vue";
import { describe, expect, it } from "vitest";
import FileDetailsPanel from "../src/components/file-browser/FileDetailsPanel.vue";
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

describe("FileDetailsPanel.vue", () => {
  it("shows a placeholder when nothing is active", () => {
    render(FileDetailsPanel as any, { props: {} });

    expect(
      screen.getByText("选中文件或文件夹后，这里会显示详细信息"),
    ).toBeInTheDocument();
    expect(screen.queryByText("类型")).toBeNull();
  });

  it("renders metadata for a file", () => {
    render(FileDetailsPanel as any, {
      props: {
        item: file({ name: "main.ts", path: "src/main.ts", size_bytes: 2048 }),
      },
    });

    expect(screen.getByText("main.ts")).toBeInTheDocument();
    expect(screen.getByText("/src/main.ts")).toBeInTheDocument();
    expect(screen.getByText("代码文件")).toBeInTheDocument();
    expect(screen.getByText("2.0 KB")).toBeInTheDocument();
    expect(screen.getByText("修改时间")).toBeInTheDocument();
    expect(screen.getByText("创建时间")).toBeInTheDocument();
  });

  it("offers preview for files but open for folders", async () => {
    const { unmount } = render(FileDetailsPanel as any, {
      props: { item: file({ name: "main.ts" }) },
    });
    expect(screen.getByText("预览")).toBeInTheDocument();
    expect(screen.queryByText("打开")).toBeNull();
    unmount();

    render(FileDetailsPanel as any, {
      props: {
        item: file({ name: "docs", path: "docs", kind: "directory" }),
      },
    });
    expect(screen.getByText("打开")).toBeInTheDocument();
    expect(screen.queryByText("预览")).toBeNull();
  });

  it("emits the matching action for each button", async () => {
    const { emitted } = render(FileDetailsPanel as any, {
      props: { item: file({ name: "main.ts" }) },
    });

    const actions: [string, string][] = [
      ["预览", "preview"],
      ["下载", "download"],
      ["分享", "share"],
      ["历史版本", "view-history"],
      ["重命名", "rename"],
      ["移动", "move"],
      ["删除", "delete"],
    ];

    for (const [label, event] of actions) {
      await fireEvent.click(screen.getByText(label));
      expect(emitted()[event]).toHaveLength(1);
    }
  });

  it("includes the last commit message when present", () => {
    render(FileDetailsPanel as any, {
      props: {
        item: file({ name: "main.ts", lastCommit: { message: "更新示例" } }),
      },
    });

    expect(screen.getByText("最近提交")).toBeInTheDocument();
    expect(screen.getByText("更新示例")).toBeInTheDocument();
  });
});
