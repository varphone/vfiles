import { render } from "@testing-library/vue";
import { describe, expect, it } from "vitest";
import FileSkeleton from "../src/components/file-browser/FileSkeleton.vue";

describe("FileSkeleton.vue", () => {
  it("renders list rows and announces loading", () => {
    const { container, getByRole } = render(FileSkeleton as any, {
      props: { variant: "list", count: 4 },
    });

    expect(container.querySelectorAll(".file-skeleton-row")).toHaveLength(4);
    expect(container.querySelectorAll(".file-skeleton-card")).toHaveLength(0);

    const status = getByRole("status");
    expect(status).toHaveAttribute("aria-busy", "true");
    expect(status.textContent).toContain("加载中");
  });

  it("renders grid cards for the grid variant", () => {
    const { container } = render(FileSkeleton as any, {
      props: { variant: "grid" },
    });

    expect(container.querySelectorAll(".file-skeleton-card")).toHaveLength(8);
    expect(container.querySelectorAll(".file-skeleton-row")).toHaveLength(0);
  });

  it("defaults to six list rows", () => {
    const { container } = render(FileSkeleton as any);

    expect(container.querySelectorAll(".file-skeleton-row")).toHaveLength(6);
  });
});
