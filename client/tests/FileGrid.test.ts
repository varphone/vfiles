import { describe, expect, it, vi } from "vitest";
import { createPinia, setActivePinia } from "pinia";
import { render, screen } from "@testing-library/vue";
import FileGrid from "../src/components/file-browser/FileGrid.vue";
import type { FileInfo } from "../src/types";

function buildFile(overrides: Partial<FileInfo> = {}): FileInfo {
  return {
    id: "file-id",
    name: "entry",
    path: "entry",
    kind: "file",
    created_at: "2026-05-29T00:00:00.000Z",
    updated_at: "2026-05-29T00:00:00.000Z",
    ...overrides,
  };
}

function renderGrid(files: FileInfo[]) {
  return render(FileGrid as any, {
    global: { plugins: [createPinia()] },
    props: {
      files,
      selectMode: false,
      selectedPaths: new Set<string>(),
    },
  });
}

describe("FileGrid.vue", () => {
  it("renders a card for every entry with name and size metadata", () => {
    setActivePinia(createPinia());
    const { container } = renderGrid([
      buildFile({
        id: "a",
        name: "photo.png",
        path: "photo.png",
        size_bytes: 2048,
      }),
      buildFile({
        id: "b",
        name: "docs",
        path: "docs",
        kind: "directory",
      }),
    ]);

    expect(screen.getByText("photo.png")).toBeInTheDocument();
    expect(screen.getByText("docs")).toBeInTheDocument();
    expect(screen.getByText("2.0 KB")).toBeInTheDocument();
    // 图片走服务端缩略图接口，且带上请求尺寸。
    const image = container.querySelector<HTMLImageElement>(
      ".file-card-thumb-image",
    );
    expect(image).not.toBeNull();
    const src = image!.getAttribute("src") || "";
    expect(src).toContain("/api/files/thumbnail?");
    expect(src).toContain("path=photo.png");
    expect(src).toContain("size=");
    // 目录不请求缩略图。
    expect(container.querySelectorAll(".file-card-thumb-image")).toHaveLength(
      1,
    );
  });

  it("shows a checkbox per card in selection mode", () => {
    setActivePinia(createPinia());
    render(FileGrid as any, {
      global: { plugins: [createPinia()] },
      props: {
        files: [buildFile({ name: "a.txt" })],
        selectMode: true,
        selectedPaths: new Set<string>(),
      },
    });

    expect(screen.getByLabelText("选择 a.txt")).toBeInTheDocument();
  });

  it("opens the context menu on long press in the grid", () => {
    vi.useFakeTimers();
    try {
      const { emitted } = render(FileGrid as any, {
        global: { plugins: [createPinia()] },
        props: {
          files: [buildFile({ id: "a", name: "a.png", path: "a.png" })],
          selectMode: false,
          selectedPaths: new Set<string>(),
        },
      });

      const card = document.querySelector(".file-card")!;
      const touchStart = new Event("touchstart", { bubbles: true }) as any;
      touchStart.touches = [{ clientX: 12, clientY: 24 }];
      card.dispatchEvent(touchStart);

      vi.advanceTimersByTime(600);

      const events = emitted()["context-menu"] as Array<
        [{ file: { path: string }; x: number; y: number }]
      >;
      expect(events).toHaveLength(1);
      expect(events[0][0]).toMatchObject({ x: 12, y: 24 });
    } finally {
      vi.useRealTimers();
    }
  });
});
