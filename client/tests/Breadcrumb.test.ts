import { render, screen, fireEvent, within } from "@testing-library/vue";
import { describe, expect, it } from "vitest";
import Breadcrumb from "../src/components/file-browser/Breadcrumb.vue";
import type { FileInfo } from "../src/types";

const breadcrumbs = [
  { name: "根目录", path: "" },
  { name: "docs", path: "docs" },
  { name: "nested", path: "docs/nested" },
];

function directory(name: string): FileInfo {
  return {
    id: name,
    name,
    path: `docs/nested/${name}`,
    kind: "directory",
    created_at: "2026-01-01T00:00:00.000Z",
  };
}

describe("Breadcrumb.vue", () => {
  it("renders every path segment", () => {
    render(Breadcrumb as any, { props: { breadcrumbs } });

    expect(screen.getByText("根目录")).toBeInTheDocument();
    expect(screen.getByText("docs")).toBeInTheDocument();
    expect(screen.getByText("nested")).toBeInTheDocument();
  });

  it("emits navigate when a segment is clicked", async () => {
    const { emitted } = render(Breadcrumb as any, {
      props: { breadcrumbs },
    });

    await fireEvent.click(screen.getByText("docs"));
    expect(emitted()["navigate"]?.[0]).toEqual(["docs"]);

    await fireEvent.click(screen.getByText("根目录"));
    expect(emitted()["navigate"]?.[1]).toEqual([""]);
  });

  it("shows current-directory subfolders in a dropdown", async () => {
    const { emitted } = render(Breadcrumb as any, {
      props: {
        breadcrumbs,
        directories: [directory("images"), directory("reports")],
      },
    });

    // 未展开时不渲染子目录
    expect(screen.queryByText("images")).not.toBeInTheDocument();

    await fireEvent.click(
      screen.getByRole("button", { name: "展开当前目录的子文件夹" }),
    );
    expect(screen.getByText("images")).toBeInTheDocument();
    expect(screen.getByText("reports")).toBeInTheDocument();

    await fireEvent.click(screen.getByText("reports"));
    const navigations = emitted()["navigate"] as Array<[string]>;
    expect(navigations[navigations.length - 1]).toEqual([
      "docs/nested/reports",
    ]);

    // 选择后菜单关闭
    expect(screen.queryByText("images")).not.toBeInTheDocument();
  });

  it("keeps the dropdown trigger even without subfolders", async () => {
    // 没有子目录时也保留入口，点击后给出「没有子文件夹」提示，
    // 避免用户以为按钮坏了
    render(Breadcrumb as any, { props: { breadcrumbs, directories: [] } });

    const trigger = screen.getByRole("button", {
      name: "展开当前目录的子文件夹",
    });
    await fireEvent.click(trigger);

    expect(screen.getByText("当前目录没有子文件夹")).toBeInTheDocument();
  });

  it("renders the dropdown outside the scrollable breadcrumb list", async () => {
    const { container } = render(Breadcrumb as any, {
      props: { breadcrumbs, directories: [directory("docs")] },
    });

    await fireEvent.click(
      screen.getByRole("button", { name: "展开当前目录的子文件夹" }),
    );

    // 面包屑列表是横向滚动容器（overflow-x: auto），下拉若留在里面会被裁剪；
    // 因此菜单必须 Teleport 到 body 之外的位置。
    const menu = screen.getByRole("menu", { name: "目录树" });
    expect(container.querySelector(".path-bar-list")?.contains(menu)).toBe(
      false,
    );
    expect(menu.closest(".path-bar-menu")).not.toBeNull();
  });

  it("keeps the dropdown open when clicking inside it", async () => {
    render(Breadcrumb as any, {
      props: { breadcrumbs, directories: [directory("docs")] },
    });

    await fireEvent.click(
      screen.getByRole("button", { name: "展开当前目录的子文件夹" }),
    );
    // 展开箭头不导航，点击后菜单应保持打开
    const menu = screen.getByRole("menu", { name: "目录树" });
    await fireEvent.click(within(menu).getByLabelText("展开 docs"));

    expect(screen.getByRole("menu", { name: "目录树" })).toBeInTheDocument();
  });

  it("closes the dropdown on an outside click", async () => {
    render(Breadcrumb as any, {
      props: { breadcrumbs, directories: [directory("images")] },
    });

    await fireEvent.click(
      screen.getByRole("button", { name: "展开当前目录的子文件夹" }),
    );
    expect(screen.getByText("images")).toBeInTheDocument();

    await fireEvent.click(document.body);
    expect(screen.queryByText("images")).not.toBeInTheDocument();
  });

  it("accepts drops on a path segment for drag-and-drop moves", async () => {
    const { emitted } = render(Breadcrumb as any, { props: { breadcrumbs } });

    const segment = screen.getByText("docs");
    await fireEvent.dragOver(segment);
    await fireEvent.drop(segment);

    expect(emitted()["drop"]?.[0]).toEqual(["docs"]);
  });
});
