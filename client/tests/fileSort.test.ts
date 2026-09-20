import { describe, expect, it } from "vitest";
import {
  DEFAULT_SORT_STATE,
  sortBrowserItems,
  sortFiles,
  type SortState,
} from "../src/utils/fileSort";
import type { FileInfo } from "../src/types";

function file(overrides: Partial<FileInfo> & { name: string }): FileInfo {
  return {
    id: overrides.name,
    path: overrides.name,
    kind: "file",
    created_at: "2026-01-01T00:00:00.000Z",
    updated_at: "2026-01-01T00:00:00.000Z",
    ...overrides,
  };
}

function dir(name: string, overrides: Partial<FileInfo> = {}): FileInfo {
  return file({ name, kind: "directory", ...overrides });
}

function state(overrides: Partial<SortState> = {}): SortState {
  return { ...DEFAULT_SORT_STATE, ...overrides };
}

describe("sortFiles", () => {
  it("sorts names naturally and keeps folders first by default", () => {
    const items = [
      file({ name: "file10.txt" }),
      dir("zebra"),
      file({ name: "file2.txt" }),
      dir("alpha"),
    ];

    const sorted = sortFiles(items, state());
    expect(sorted.map((f) => f.name)).toEqual([
      "alpha",
      "zebra",
      "file2.txt",
      "file10.txt",
    ]);
  });

  it("does not mutate the input array", () => {
    const items = [file({ name: "b" }), file({ name: "a" })];
    const snapshot = items.map((f) => f.name);
    sortFiles(items, state());
    expect(items.map((f) => f.name)).toEqual(snapshot);
  });

  it("sorts by size with folders still on top", () => {
    const items = [
      file({ name: "big", size_bytes: 900 }),
      dir("folder", { size_bytes: 999999 }),
      file({ name: "small", size_bytes: 10 }),
    ];

    const sorted = sortFiles(
      items,
      state({ field: "size", direction: "desc" }),
    );
    expect(sorted.map((f) => f.name)).toEqual(["folder", "big", "small"]);
  });

  it("sorts by modified time and reverses inside groups", () => {
    const items = [
      file({ name: "old", updated_at: "2026-01-01T00:00:00.000Z" }),
      file({ name: "new", updated_at: "2026-06-01T00:00:00.000Z" }),
      file({ name: "mid", updated_at: "2026-03-01T00:00:00.000Z" }),
    ];

    const asc = sortFiles(items, state({ field: "modified" }));
    expect(asc.map((f) => f.name)).toEqual(["old", "mid", "new"]);

    const desc = sortFiles(
      items,
      state({ field: "modified", direction: "desc" }),
    );
    expect(desc.map((f) => f.name)).toEqual(["new", "mid", "old"]);
  });

  it("groups by extension when sorting by type", () => {
    const items = [
      file({ name: "b.txt" }),
      file({ name: "a.png" }),
      file({ name: "c.txt" }),
      file({ name: "d.zip" }),
    ];

    const sorted = sortFiles(items, state({ field: "type" }));
    expect(sorted.map((f) => f.name)).toEqual([
      "a.png",
      "b.txt",
      "c.txt",
      "d.zip",
    ]);
  });

  it("can interleave folders when foldersFirst is disabled", () => {
    const items = [dir("m"), file({ name: "a" })];
    const sorted = sortFiles(items, state({ foldersFirst: false }));
    expect(sorted.map((f) => f.name)).toEqual(["a", "m"]);
  });
});

describe("sortBrowserItems", () => {
  it("keeps shortcut entries at the top and sorts real entries", () => {
    const items = [
      { ...dir("."), uiRole: "self" as const },
      { ...dir(".."), uiRole: "parent" as const },
      file({ name: "b.txt" }),
      file({ name: "a.txt" }),
    ];

    const sorted = sortBrowserItems(items, state());
    expect(sorted.map((f) => f.name)).toEqual([".", "..", "a.txt", "b.txt"]);
  });

  it("preserves the relative order of shortcut entries", () => {
    const items = [
      { ...dir("z"), uiRole: "parent" as const },
      file({ name: "a.txt" }),
      { ...dir("."), uiRole: "self" as const },
    ];

    const sorted = sortBrowserItems(items, state());
    expect(sorted.map((f) => f.name)).toEqual(["z", ".", "a.txt"]);
  });
});
