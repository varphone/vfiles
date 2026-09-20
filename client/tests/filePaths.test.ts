import { describe, expect, it } from "vitest";
import {
  buildChildPath,
  buildSiblingPath,
  isSafeDirName,
  normalizeTargetDirectory,
  parentDirectoryPath,
  planMoveOperations,
  resolveMoveTargetPath,
} from "../src/utils/filePaths";
import type { FileInfo } from "../src/types";

function entry(overrides: Partial<FileInfo> & { path: string }): FileInfo {
  return {
    id: overrides.path,
    name: overrides.path.split("/").pop() || overrides.path,
    kind: "file",
    created_at: "2026-01-01T00:00:00.000Z",
    ...overrides,
  };
}

describe("isSafeDirName", () => {
  it("accepts plain names and rejects paths/blank/dots", () => {
    expect(isSafeDirName("docs")).toBe(true);
    expect(isSafeDirName("  report ")).toBe(true);
    expect(isSafeDirName("")).toBe(false);
    expect(isSafeDirName("   ")).toBe(false);
    expect(isSafeDirName(".")).toBe(false);
    expect(isSafeDirName("..")).toBe(false);
    expect(isSafeDirName("a/b")).toBe(false);
    expect(isSafeDirName("a\\b")).toBe(false);
  });
});

describe("path builders", () => {
  it("joins and splits paths without leading/trailing slashes", () => {
    expect(buildChildPath("", "a")).toBe("a");
    expect(buildChildPath("docs", "a")).toBe("docs/a");
    expect(parentDirectoryPath("docs/nested/a.txt")).toBe("docs/nested");
    expect(parentDirectoryPath("a.txt")).toBe("");
    expect(buildSiblingPath("docs/a.txt", "b.txt")).toBe("docs/b.txt");
    expect(buildSiblingPath("a.txt", "b.txt")).toBe("b.txt");
  });

  it("normalizes user supplied target directories", () => {
    expect(normalizeTargetDirectory("  /docs/nested/ ")).toBe("docs/nested");
    expect(normalizeTargetDirectory("docs\\nested")).toBe("docs/nested");
    expect(normalizeTargetDirectory("")).toBe("");
  });
});

describe("resolveMoveTargetPath", () => {
  it("computes the destination and rejects no-op moves", () => {
    const file = entry({ path: "docs/a.txt" });
    expect(resolveMoveTargetPath(file, "photos")).toBe("photos/a.txt");
    expect(() => resolveMoveTargetPath(file, "docs")).toThrow("目标目录未变化");
  });

  it("rejects moving a directory into itself", () => {
    const dir = entry({ path: "docs", kind: "directory" });
    expect(() => resolveMoveTargetPath(dir, "docs")).toThrow("自身或其子目录");
    expect(() => resolveMoveTargetPath(dir, "docs/nested")).toThrow(
      "自身或其子目录",
    );
    expect(resolveMoveTargetPath(dir, "photos")).toBe("photos/docs");
  });
});

describe("planMoveOperations", () => {
  it("plans each move", () => {
    const ops = planMoveOperations(
      [entry({ path: "a.txt" }), entry({ path: "b.txt" })],
      "docs",
    );
    expect(ops.map((op) => op.to)).toEqual(["docs/a.txt", "docs/b.txt"]);
  });

  it("rejects collisions from the batch itself or the target directory", () => {
    expect(() =>
      planMoveOperations(
        [entry({ path: "x/a.txt" }), entry({ path: "y/a.txt" })],
        "docs",
      ),
    ).toThrow("会产生重名项");

    expect(() =>
      planMoveOperations([entry({ path: "a.txt" })], "docs", [
        { path: "docs/a.txt" },
      ]),
    ).toThrow("已存在同名项目");
  });
});
