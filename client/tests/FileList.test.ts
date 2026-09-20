import { fireEvent, render, screen } from "@testing-library/vue";
import { describe, expect, it, vi } from "vitest";
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

  it("emits modifier-select for shift/ctrl clicks", async () => {
    const { emitted } = render(FileList as any, {
      props: {
        desktop: true,
        selectMode: false,
        selectedPaths: new Set<string>(),
        files: [buildFile({ id: "a", name: "a.txt", path: "a.txt" })],
      },
    });

    const row = screen.getByText("a.txt").closest("tr")!;
    await fireEvent.click(row, { shiftKey: true });
    await fireEvent.click(row, { ctrlKey: true });

    const events = emitted()["modifier-select"] as Array<
      [{ file: { path: string }; shift: boolean; meta: boolean }]
    >;
    expect(events).toHaveLength(2);
    expect(events[0][0].shift).toBe(true);
    expect(events[0][0].meta).toBe(false);
    expect(events[1][0].meta).toBe(true);
  });

  it("emits context-menu with the pointer position", async () => {
    const { emitted } = render(FileList as any, {
      props: {
        desktop: true,
        selectMode: false,
        selectedPaths: new Set<string>(),
        files: [buildFile({ id: "a", name: "a.txt", path: "a.txt" })],
      },
    });

    const row = screen.getByText("a.txt").closest("tr")!;
    await fireEvent.contextMenu(row, { clientX: 120, clientY: 80 });

    const events = emitted()["context-menu"] as Array<
      [{ file: { path: string }; x: number; y: number }]
    >;
    expect(events).toHaveLength(1);
    expect(events[0][0].file.path).toBe("a.txt");
    expect(events[0][0].x).toBe(120);
    expect(events[0][0].y).toBe(80);
  });

  it("emits drag events and accepts drops on directories", async () => {
    const { emitted } = render(FileList as any, {
      props: {
        desktop: true,
        selectMode: false,
        selectedPaths: new Set<string>(),
        files: [
          buildFile({ id: "d", name: "docs", path: "docs", kind: "directory" }),
          buildFile({ id: "a", name: "a.txt", path: "a.txt" }),
        ],
      },
    });

    const fileRow = screen.getByText("a.txt").closest("tr")!;
    await fireEvent.dragStart(fileRow);
    const starts = emitted()["drag-start"] as Array<[{ path: string }]>;
    expect(starts).toHaveLength(1);
    expect(starts[0][0].path).toBe("a.txt");

    const dirRow = screen.getByText("docs").closest("tr")!;
    await fireEvent.dragOver(dirRow);
    expect(dirRow).toHaveClass("drop-target");

    await fireEvent.drop(dirRow);
    expect(emitted()["drop-on-folder"]?.[0]).toEqual(["docs"]);
    expect(dirRow).not.toHaveClass("drop-target");
  });

  it("ignores drops on non-directories", async () => {
    const { emitted } = render(FileList as any, {
      props: {
        desktop: true,
        selectMode: false,
        selectedPaths: new Set<string>(),
        files: [buildFile({ id: "a", name: "a.txt", path: "a.txt" })],
      },
    });

    const row = screen.getByText("a.txt").closest("tr")!;
    await fireEvent.dragOver(row);
    await fireEvent.drop(row);

    expect(emitted()["drop-on-folder"]).toBeUndefined();
    expect(row).not.toHaveClass("drop-target");
  });

  it("opens the context menu on long press in the mobile list", () => {
    vi.useFakeTimers();
    try {
      const { emitted } = render(FileList as any, {
        props: {
          desktop: false,
          selectMode: false,
          selectedPaths: new Set<string>(),
          files: [buildFile({ id: "a", name: "a.txt", path: "a.txt" })],
        },
      });

      const item = screen.getByText("a.txt").closest(".file-item")!;
      const touchStart = new Event("touchstart", { bubbles: true }) as any;
      touchStart.touches = [{ clientX: 30, clientY: 40 }];
      item.dispatchEvent(touchStart);

      vi.advanceTimersByTime(600);

      const events = emitted()["context-menu"] as Array<
        [{ file: { path: string }; x: number; y: number }]
      >;
      expect(events).toHaveLength(1);
      expect(events[0][0].file.path).toBe("a.txt");
      expect(events[0][0].x).toBe(30);
      expect(events[0][0].y).toBe(40);

      // 长按后的 click 不应再触发选择/打开
      item.dispatchEvent(new Event("click", { bubbles: true }));
      expect(emitted()["click"]).toBeUndefined();
    } finally {
      vi.useRealTimers();
    }
  });

  it("cancels the long press when the finger moves", () => {
    vi.useFakeTimers();
    try {
      const { emitted } = render(FileList as any, {
        props: {
          desktop: false,
          selectMode: false,
          selectedPaths: new Set<string>(),
          files: [buildFile({ id: "a", name: "a.txt", path: "a.txt" })],
        },
      });

      const item = screen.getByText("a.txt").closest(".file-item")!;
      const touchStart = new Event("touchstart", { bubbles: true }) as any;
      touchStart.touches = [{ clientX: 10, clientY: 10 }];
      item.dispatchEvent(touchStart);
      item.dispatchEvent(new Event("touchmove", { bubbles: true }));

      vi.advanceTimersByTime(600);

      expect(emitted()["context-menu"]).toBeUndefined();
    } finally {
      vi.useRealTimers();
    }
  });
});
