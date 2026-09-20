import { describe, expect, it } from "vitest";
import {
  fileIconKind,
  fileKindLabel,
  formatRelativeDate,
} from "../src/utils/filePresentation";
import type { FileInfo } from "../src/types";

const NOW = new Date(2026, 8, 20, 21, 3, 0); // 2026-09-20 21:03 本地时间

function iso(year: number, month: number, day: number, hour = 9, minute = 0) {
  return new Date(year, month - 1, day, hour, minute).toISOString();
}

describe("formatRelativeDate", () => {
  it("shows the time for today", () => {
    expect(formatRelativeDate(iso(2026, 9, 20, 8, 5), NOW)).toBe("今天 08:05");
  });

  it("labels yesterday explicitly", () => {
    expect(formatRelativeDate(iso(2026, 9, 19, 22, 40), NOW)).toBe(
      "昨天 22:40",
    );
  });

  it("uses a day count within the last week", () => {
    expect(formatRelativeDate(iso(2026, 9, 17), NOW)).toBe("3 天前");
    expect(formatRelativeDate(iso(2026, 9, 14), NOW)).toBe("6 天前");
  });

  it("falls back to an absolute date for older entries", () => {
    const label = formatRelativeDate(iso(2026, 9, 1), NOW);
    expect(label).not.toContain("天前");
    expect(label).toContain("2026");
  });

  it("keeps missing or invalid values readable", () => {
    expect(formatRelativeDate(undefined, NOW)).toBe("--");
    expect(formatRelativeDate("not-a-date", NOW)).toBe("not-a-date");
  });
});

function file(overrides: Partial<FileInfo>): FileInfo {
  return {
    id: overrides.name ?? "id",
    name: overrides.name ?? "file",
    path: overrides.path ?? overrides.name ?? "file",
    kind: "file",
    created_at: "2026-09-20T00:00:00.000Z",
    ...overrides,
  };
}

describe("file kind classification", () => {
  it("prefers the extension over a misleading video mime", () => {
    // 浏览器常把 .ts 识别为 video/mp2t，实际是 TypeScript 源码
    const source = file({ name: "main.ts", mime_type: "video/mp2t" });
    expect(fileKindLabel(source)).toBe("代码文件");
    expect(fileIconKind(source)).toBe("code");
  });

  it("still classifies real media by mime", () => {
    expect(
      fileIconKind(file({ name: "clip.mp4", mime_type: "video/mp4" })),
    ).toBe("video");
    expect(
      fileIconKind(file({ name: "shot.png", mime_type: "image/png" })),
    ).toBe("image");
    expect(
      fileKindLabel(file({ name: "song.mp3", mime_type: "audio/mpeg" })),
    ).toBe("音频文件");
  });

  it("falls back to text and generic labels", () => {
    expect(fileKindLabel(file({ name: "notes.md" }))).toBe("文本文档");
    expect(fileKindLabel(file({ name: "whatever.bin" }))).toBe("BIN 文件");
  });
});
