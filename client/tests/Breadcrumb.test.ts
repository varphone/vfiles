import { render, screen, fireEvent } from "@testing-library/vue";
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

  it("hides the dropdown trigger without subfolders", () => {
    render(Breadcrumb as any, { props: { breadcrumbs, directories: [] } });
    expect(
      screen.queryByRole("button", { name: "展开当前目录的子文件夹" }),
    ).not.toBeInTheDocument();
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
});
