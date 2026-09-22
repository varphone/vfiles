import { fireEvent, render } from "@testing-library/vue";
import { describe, expect, it, vi } from "vitest";
import FileItem from "../src/components/file-browser/FileItem.vue";
import type { FileInfo } from "../src/types";

vi.mock("../src/services/files.service", () => ({
  SEARCH_PAGE_SIZE: 100,
  filesService: { thumbnailUrl: () => "/thumb" },
}));

function file(overrides: Partial<FileInfo> = {}): FileInfo {
  return {
    id: "a",
    name: "a.txt",
    path: "a.txt",
    kind: "file",
    size_bytes: 1024,
    created_at: "2026-04-10T00:00:00.000Z",
    updated_at: "2026-04-10T00:00:00.000Z",
    ...overrides,
  };
}

describe("FileItem.vue row actions", () => {
  it("keeps the row menu button when the action column is hidden", async () => {
    const { container, emitted } = render(FileItem as any, {
      props: { file: file(), desktop: true, showActionColumn: false },
    });

    // 操作列关闭时不再有行内按钮组，但「⋯」入口仍在
    expect(container.querySelector(".desktop-action-buttons")).toBeNull();
    const menuButton = container.querySelector(
      ".desktop-row-menu",
    ) as HTMLButtonElement;
    expect(menuButton).not.toBeNull();
    expect(menuButton.getAttribute("aria-label")).toBe("a.txt 的操作");

    await fireEvent.click(menuButton);

    const events = emitted("contextMenu") as unknown as [
      { file: FileInfo; x: number; y: number },
    ][];
    expect(events).toHaveLength(1);
    expect(events[0][0].file.path).toBe("a.txt");
    // 菜单定位在按钮下方
    expect(events[0][0].y).toBeGreaterThanOrEqual(0);
  });

  it("keeps the hover checkbox in sync with the selection", async () => {
    const { container, rerender } = render(FileItem as any, {
      props: {
        file: file(),
        desktop: true,
        selectMode: false,
        selected: false,
      },
    });

    // 未选中：hover 出现的复选框
    const hover = container.querySelector<HTMLInputElement>(
      ".desktop-row-check input",
    )!;
    expect(hover.checked).toBe(false);

    // 选中后：复选框常驻并反映选中态（此前写死 :checked="false"，看起来永远没勾上）
    await rerender({ selected: true });
    const persistent = container.querySelector<HTMLInputElement>(
      'input[type="checkbox"]',
    )!;
    expect(persistent.checked).toBe(true);
  });

  it("offers a hover checkbox outside batch mode", async () => {
    const { container, emitted } = render(FileItem as any, {
      props: { file: file(), desktop: true, selectMode: false },
    });

    const checkbox = container.querySelector(
      'input[type="checkbox"]',
    ) as HTMLInputElement;
    expect(checkbox).not.toBeNull();
    expect(checkbox.closest("label")?.className).toContain("desktop-row-check");

    await fireEvent.change(checkbox);
    expect(emitted("toggleSelect")).toHaveLength(1);
  });

  it("renders a persistent checkbox in batch mode", () => {
    const { container } = render(FileItem as any, {
      props: { file: file(), desktop: true, selectMode: true, selected: true },
    });

    const label = container.querySelector("label");
    // 统一槽位语义：始终 desktop-row-check，批量/选中态追加 is-visible（常驻不位移）
    expect(label?.className).toContain("desktop-row-check");
    expect(label?.className).toContain("is-visible");
    const checkbox = container.querySelector(
      'input[type="checkbox"]',
    ) as HTMLInputElement;
    expect(checkbox.checked).toBe(true);
  });
});

describe("FileItem.vue mobile row meta", () => {
  it("renders size and date as plain muted text instead of pills", () => {
    const { container } = render(FileItem as any, {
      props: { file: file({ kind: "file", size_bytes: 1536 }) },
    });

    const info = container.querySelector(".file-info")!;
    expect(info.querySelector(".file-info-size")?.textContent?.trim()).toBe(
      "1.5 KB",
    );
    expect(info.querySelector(".file-info-sep")).not.toBeNull();
    expect(info.querySelector(".file-info-date")).not.toBeNull();
    // 不再使用胶囊标签
    expect(info.querySelector(".tag")).toBeNull();
  });

  it("renders the latest commit message as muted text", () => {
    const { container } = render(FileItem as any, {
      props: {
        file: file({
          lastCommit: { message: "更新说明", hash: "abc1234" },
        } as Partial<FileInfo>),
      },
    });

    const commit = container.querySelector(".file-commit-message");
    expect(commit?.textContent?.trim()).toBe("更新说明");
    expect(container.querySelector(".file-commit .tag")).toBeNull();
  });

  it("renders the drop confirmation flash from the parent-held flash prop", () => {
    // 回执态由父层持态（flash prop）：remount 后仍存活 ✓ 渲染层断言
    const { container } = render(FileItem as any, {
      props: {
        file: file({ kind: "directory" }),
        desktop: true,
        dragging: true,
        flash: true,
      },
    });
    const row = container.querySelector(
      "tr.desktop-file-row, .file-item",
    ) as HTMLElement;
    expect(row.className).toContain("is-drop-confirmed");
  });
});
