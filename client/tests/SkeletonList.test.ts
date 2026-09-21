import { render } from "@testing-library/vue";
import { describe, expect, it } from "vitest";
import SkeletonList from "../src/components/common/SkeletonList.vue";

describe("SkeletonList.vue", () => {
  it("renders the requested number of rows with an accessible status", () => {
    const { container } = render(SkeletonList as any, {
      props: { rows: 3, label: "加载用户列表" },
    });

    const status = container.querySelector('[role="status"]')!;
    expect(status.getAttribute("aria-busy")).toBe("true");
    expect(status.getAttribute("aria-label")).toBe("加载用户列表");
    expect(container.querySelectorAll(".skeleton-row")).toHaveLength(3);
    // 默认 rows 变体：图标 + 两行文本
    expect(container.querySelectorAll(".skeleton-icon")).toHaveLength(3);
  });

  it("supports folder and text-line variants", () => {
    const folders = render(SkeletonList as any, {
      props: { variant: "folders", rows: 2 },
    });
    expect(folders.container.querySelectorAll(".skeleton-icon")).toHaveLength(
      2,
    );
    expect(folders.container.querySelector(".is-lines")).toBeNull();

    const lines = render(SkeletonList as any, {
      props: { variant: "lines", rows: 5 },
    });
    expect(lines.container.querySelector(".is-lines")).not.toBeNull();
    expect(lines.container.querySelectorAll(".skeleton-row")).toHaveLength(5);
    // 文本行没有图标
    expect(lines.container.querySelectorAll(".skeleton-icon")).toHaveLength(0);
  });

  it("falls back to a generic label", () => {
    const { container } = render(SkeletonList as any);

    expect(
      container.querySelector('[role="status"]')?.getAttribute("aria-label"),
    ).toBe("加载中");
    expect(container.querySelectorAll(".skeleton-row")).toHaveLength(4);
  });
});
