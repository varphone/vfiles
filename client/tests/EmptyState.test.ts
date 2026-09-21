import { render, screen } from "@testing-library/vue";
import { describe, expect, it } from "vitest";
import { IconAlertCircle, IconFolderOpen } from "@tabler/icons-vue";
import EmptyState from "../src/components/common/EmptyState.vue";

describe("EmptyState.vue", () => {
  it("renders the illustration, title, hint and actions", () => {
    const { container } = render(EmptyState as any, {
      props: {
        icon: IconFolderOpen,
        title: "此文件夹为空",
        hint: "拖拽文件到这里上传",
      },
      slots: { actions: "<button>上传文件</button>" },
    });

    expect(screen.getByText("此文件夹为空")).toBeInTheDocument();
    expect(screen.getByText("拖拽文件到这里上传")).toBeInTheDocument();
    expect(screen.getByText("上传文件")).toBeInTheDocument();
    // 插画底座里是 svg 图标
    expect(
      container.querySelector(".empty-state-illustration svg"),
    ).not.toBeNull();
  });

  it("uses the error tone and marks compact mode", () => {
    const { container } = render(EmptyState as any, {
      props: {
        icon: IconAlertCircle,
        title: "加载失败",
        hint: "网络错误",
        tone: "error",
        compact: true,
      },
    });

    const root = container.querySelector(".empty-state")!;
    expect(root.classList.contains("is-error")).toBe(true);
    expect(root.classList.contains("is-compact")).toBe(true);
  });

  it("omits the hint and actions when not provided", () => {
    const { container } = render(EmptyState as any, {
      props: { icon: IconFolderOpen, title: "空" },
    });

    expect(container.querySelector(".empty-state-hint")).toBeNull();
    expect(container.querySelector(".empty-state-actions")).toBeNull();
  });
});
