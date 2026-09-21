import { fireEvent, render, screen } from "@testing-library/vue";
import { describe, expect, it } from "vitest";
import SearchResultToolbar from "../src/components/file-browser/SearchResultToolbar.vue";
import type { FileInfo } from "../src/types";

function file(overrides: Partial<FileInfo>): FileInfo {
  return {
    id: overrides.path ?? "id",
    name: overrides.name ?? "x",
    path: overrides.path ?? "x",
    kind: overrides.kind ?? "file",
    created_at: "2026-09-20T10:00:00.000Z",
    updated_at: "2026-09-20T10:00:00.000Z",
    ...overrides,
  };
}

const results: FileInfo[] = [
  file({ name: "项目", path: "项目", kind: "directory" }),
  file({ name: "说明.md", path: "项目/说明.md", mime_type: "text/markdown" }),
  file({ name: "图.png", path: "图片/图.png", mime_type: "image/png" }),
  file({ name: "片.mp4", path: "视频/片.mp4", mime_type: "video/mp4" }),
  file({ name: "乐.mp3", path: "音频/乐.mp3", mime_type: "audio/mpeg" }),
  file({ name: "包.zip", path: "包.zip" }),
];

function renderToolbar(overrides: Record<string, unknown> = {}) {
  return render(SearchResultToolbar as any, {
    props: {
      results,
      active: "all",
      query: "星云",
      visibleCount: results.length,
      ...overrides,
    },
  });
}

describe("SearchResultToolbar.vue", () => {
  it("summarises the result count and lists buckets with counts", () => {
    const { container } = renderToolbar();

    expect(
      container.querySelector(".search-toolbar-summary")?.textContent,
    ).toBe("找到 6 项");
    // chips 内是相邻 span，textContent 不会自动插入空格，分别取名称与数量
    const chips = Array.from(container.querySelectorAll(".search-chip")).map(
      (chip) => ({
        label: chip.querySelector("span")?.textContent?.trim(),
        count: chip.querySelector(".search-chip-count")?.textContent?.trim(),
      }),
    );
    expect(chips).toEqual([
      { label: "全部", count: "6" },
      { label: "文件夹", count: "1" },
      { label: "文档", count: "1" },
      { label: "图片", count: "1" },
      { label: "视频", count: "1" },
      { label: "音频", count: "1" },
      { label: "其它", count: "1" },
    ]);
  });

  it("emits the selected bucket and marks it active", async () => {
    const { container, emitted } = renderToolbar();

    const imageChip = Array.from(
      container.querySelectorAll<HTMLButtonElement>(".search-chip"),
    ).find((chip) => chip.textContent?.includes("图片"))!;
    await fireEvent.click(imageChip);

    expect(emitted()["update:active"]?.[0]).toEqual(["image"]);
  });

  it("shows the filtered total and hides empty buckets", () => {
    const { container } = renderToolbar({
      results: [results[0], results[1]],
      active: "document",
      visibleCount: 1,
    });

    expect(
      container.querySelector(".search-toolbar-summary")?.textContent,
    ).toBe("找到 2 项 · 文档 1 项");
    const labels = Array.from(container.querySelectorAll(".search-chip")).map(
      (chip) => ({
        label: chip.querySelector("span")?.textContent?.trim(),
        count: chip.querySelector(".search-chip-count")?.textContent?.trim(),
      }),
    );
    expect(labels).toEqual([
      { label: "全部", count: "2" },
      { label: "文件夹", count: "1" },
      { label: "文档", count: "1" },
    ]);
  });

  it("hides the chip row when everything is one type", () => {
    const { container } = renderToolbar({ results: [results[2]] });

    expect(container.querySelector(".search-toolbar-chips")).toBeNull();
    expect(
      container.querySelector(".search-toolbar-summary")?.textContent,
    ).toBe("找到 1 项");
  });

  it("notes how many results are currently rendered", () => {
    renderToolbar({ visibleCount: 4 });

    expect(screen.getByText("找到 6 项 · 已显示 4 项")).toBeInTheDocument();
  });
});
