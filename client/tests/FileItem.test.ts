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
    expect(label?.className).not.toContain("desktop-row-check");
    const checkbox = container.querySelector(
      'input[type="checkbox"]',
    ) as HTMLInputElement;
    expect(checkbox.checked).toBe(true);
  });
});
