import { render, screen } from "@testing-library/vue";
import { describe, expect, it } from "vitest";
import UploadDropOverlay from "../src/components/file-browser/UploadDropOverlay.vue";

describe("UploadDropOverlay.vue", () => {
  it("is hidden until files are dragged in", () => {
    const { container } = render(UploadDropOverlay as any, {
      props: { visible: false, targetLabel: "根目录" },
    });

    expect(container.querySelector(".upload-drop-overlay")).toBeNull();
  });

  it("announces the target directory while dragging", () => {
    render(UploadDropOverlay as any, {
      props: { visible: true, targetLabel: "设计稿" },
    });

    expect(screen.getByRole("status")).toBeInTheDocument();
    expect(screen.getByText("松开即可上传")).toBeInTheDocument();
    expect(screen.getByText("设计稿")).toBeInTheDocument();
  });
});
