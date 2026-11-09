import { describe, expect, it } from "vitest";
import {
  firstScopeRoot,
  isPathWithinRoots,
  normalizeScopePath,
  normalizeScopeRoots,
  scopeRootLabel,
  scopedParentPath,
  sliceBreadcrumbsFromScopeRoot,
} from "../src/utils/sessionScope";

describe("sessionScope", () => {
  it("normalizes slashes and duplicate roots", () => {
    expect(normalizeScopePath("/docs//design/")).toBe("docs/design");
    expect(normalizeScopeRoots([" /docs/ ", "docs", "", "images"])).toEqual([
      "docs",
      "images",
    ]);
  });

  it("matches roots by path segment rather than string prefix", () => {
    expect(isPathWithinRoots("docs", ["docs"])).toBe(true);
    expect(isPathWithinRoots("docs/readme.md", ["docs"])).toBe(true);
    expect(isPathWithinRoots("docs2/readme.md", ["docs"])).toBe(false);
    expect(isPathWithinRoots("", ["docs"])).toBe(false);
    // 没有范围根 = 全量会话
    expect(isPathWithinRoots("anything", [])).toBe(true);
  });

  it("stops parent navigation at the scope root", () => {
    expect(scopedParentPath("docs/sub/file.txt", ["docs"])).toBe("docs/sub");
    expect(scopedParentPath("docs/sub", ["docs"])).toBe("docs");
    expect(scopedParentPath("docs", ["docs"])).toBeNull();
    expect(scopedParentPath("other", ["docs"])).toBeNull();
    // 全量会话可以回到命名空间根
    expect(scopedParentPath("docs", [])).toBe("");
  });

  it("hides breadcrumbs above the scope root", () => {
    const crumbs = [
      { name: "根目录", path: "" },
      { name: "docs", path: "docs" },
      { name: "sub", path: "docs/sub" },
    ];
    expect(sliceBreadcrumbsFromScopeRoot(crumbs, ["docs"])).toEqual([
      { name: "docs", path: "docs" },
      { name: "sub", path: "docs/sub" },
    ]);
    expect(sliceBreadcrumbsFromScopeRoot(crumbs, [])).toEqual(crumbs);
  });

  it("uses the first root and its short label", () => {
    expect(firstScopeRoot(["docs/design", "images"])).toBe("docs/design");
    expect(scopeRootLabel("docs/design")).toBe("design");
  });
});
