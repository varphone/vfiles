import { fireEvent, render, screen } from "@testing-library/vue";
import { describe, expect, it } from "vitest";
import FileList from "../src/components/file-browser/FileList.vue";
import type { FileInfo } from "../src/types";

function buildFile(overrides: Record<string, unknown> = {}) {
  return {
    id: "file-id",
    name: "entry",
    path: "entry",
    kind: "file",
    created_at: "2026-05-29T00:00:00.000Z",
    updated_at: "2026-05-29T00:00:00.000Z",
    ...overrides,
  };
}

describe("FileList.vue", () => {
  it("renders directory and shortcut names as links in desktop mode", () => {
    render(FileList as any, {
      props: {
        desktop: true,
        selectMode: false,
        selectedPaths: new Set<string>(),
        files: [
          buildFile({
            id: "self-id",
            name: ".",
            path: "__vfiles_shortcut_self__:root",
            kind: "directory",
            uiRole: "self",
          }),
          buildFile({
            id: "parent-id",
            name: "..",
            path: "__vfiles_shortcut_parent__:root",
            kind: "directory",
            uiRole: "parent",
          }),
          buildFile({
            id: "dir-id",
            name: "docs",
            path: "docs",
            kind: "directory",
          }),
          buildFile({
            id: "file-id",
            name: "readme.txt",
            path: "readme.txt",
            kind: "file",
          }),
        ],
      },
    });

    expect(screen.getByRole("link", { name: "." })).toHaveClass(
      "has-text-link",
    );
    expect(screen.getByRole("link", { name: ".." })).toHaveClass(
      "has-text-link",
    );
    expect(screen.getByRole("link", { name: "docs" })).toHaveClass(
      "has-text-link",
    );
    expect(
      screen.queryByRole("link", { name: "readme.txt" }),
    ).not.toBeInTheDocument();
  });

  it("emits sort-change when a column header is clicked", async () => {
    const { emitted } = render(FileList as any, {
      props: {
        desktop: true,
        selectMode: false,
        selectedPaths: new Set<string>(),
        sortField: "name",
        sortDirection: "asc",
        files: [buildFile({ name: "readme.txt" })],
      },
    });

    await fireEvent.click(screen.getByRole("button", { name: /修改日期/ }));
    expect(emitted()["sort-change"]?.[0]).toEqual(["modified"]);

    const nameHeader = screen.getByRole("columnheader", { name: /名称/ });
    expect(nameHeader).toHaveAttribute("aria-sort", "ascending");
    const sizeHeader = screen.getByRole("columnheader", { name: /大小/ });
    expect(sizeHeader).toHaveAttribute("aria-sort", "none");
  });

  it("shows an indeterminate select-all checkbox for a partial selection", async () => {
    const files: FileInfo[] = [
      buildFile({ id: "a", name: "a.txt", path: "a.txt" }) as FileInfo,
      buildFile({ id: "b", name: "b.txt", path: "b.txt" }) as FileInfo,
    ];

    const { rerender } = render(FileList as any, {
      props: {
        desktop: true,
        selectMode: true,
        selectedPaths: new Set<string>(["a.txt"]),
        files,
      },
    });

    const checkbox = screen.getByLabelText("全选当前视图") as HTMLInputElement;
    expect(checkbox.indeterminate).toBe(true);
    expect(checkbox.checked).toBe(false);

    await rerender({
      desktop: true,
      selectMode: true,
      selectedPaths: new Set<string>(["a.txt", "b.txt"]),
      files,
    });
    const allChecked = screen.getByLabelText(
      "全选当前视图",
    ) as HTMLInputElement;
    expect(allChecked.indeterminate).toBe(false);
    expect(allChecked.checked).toBe(true);
  });

  it("emits toggle-select-all from the header checkbox", async () => {
    const { emitted } = render(FileList as any, {
      props: {
        desktop: true,
        selectMode: true,
        selectedPaths: new Set<string>(),
        files: [buildFile({ name: "a.txt", path: "a.txt" })],
      },
    });

    await fireEvent.click(screen.getByLabelText("全选当前视图"));
    expect(emitted()["toggle-select-all"]).toHaveLength(1);
  });
});
