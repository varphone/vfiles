import { render, screen } from "@testing-library/vue";
import { describe, expect, it } from "vitest";
import ProgressBar from "../src/components/common/ProgressBar.vue";

describe("ProgressBar.vue", () => {
  it("renders a determinate progress element with the value and label", () => {
    render(ProgressBar as any, {
      props: { mode: "determinate", value: 42, label: "上传 main.ts" },
    });

    const bar = screen.getByRole("progressbar", { name: "上传 main.ts" });
    expect(bar).toHaveAttribute("value", "42");
    expect(bar).toHaveAttribute("max", "100");
    expect(bar).toHaveClass("vf-progress");
    expect(bar).not.toHaveClass("is-indeterminate");
  });

  it("renders an indeterminate bar when the total is unknown", () => {
    render(ProgressBar as any, {
      props: { mode: "indeterminate", label: "下载 main.ts" },
    });

    const bar = screen.getByRole("progressbar", { name: "下载 main.ts" });
    expect(bar).not.toHaveAttribute("value");
    expect(bar).toHaveClass("is-indeterminate");
  });

  it("does not repeat the percentage as text", () => {
    const { container } = render(ProgressBar as any, {
      props: { mode: "determinate", value: 66 },
    });

    // 百分比由调用方就近展示，避免同一数字出现两次
    expect(container.querySelector("progress")?.textContent?.trim()).toBe("");
  });
});
