import { beforeEach, describe, expect, it, vi } from "vitest";
import { createPinia, setActivePinia } from "pinia";
import { ref } from "vue";
import { useMoveDialog } from "../src/composables/useMoveDialog";
import type { FileInfo } from "../src/types";

const { getFilesMock, movePathMock } = vi.hoisted(() => ({
  getFilesMock: vi.fn(async () => [] as unknown[]),
  movePathMock: vi.fn(async () => ({})),
}));

vi.mock("../src/services/files.service", () => ({
  filesService: {
    getFiles: getFilesMock,
    movePath: movePathMock,
  },
}));

function entry(path: string, kind: "file" | "directory" = "file"): FileInfo {
  return {
    id: path,
    name: path.split("/").pop() || path,
    path,
    kind,
    created_at: "2026-01-01T00:00:00.000Z",
  };
}

function setup() {
  const currentPath = ref("docs");
  const refreshAfterMutation = vi.fn(async () => {});
  const replaceSelectedPath = vi.fn();
  let activePath = "";
  const setActivePath = vi.fn((path: string) => {
    activePath = path;
  });
  const clearSelection = vi.fn();

  const dialog = useMoveDialog({
    currentPath,
    refreshAfterMutation,
    replaceSelectedPath,
    getActivePath: () => activePath,
    setActivePath,
    clearSelection,
  });

  return {
    dialog,
    currentPath,
    refreshAfterMutation,
    replaceSelectedPath,
    setActivePath,
    clearSelection,
    getActivePath: () => activePath,
  };
}

describe("useMoveDialog", () => {
  beforeEach(() => {
    setActivePinia(createPinia());
    getFilesMock.mockReset();
    movePathMock.mockReset();
    getFilesMock.mockResolvedValue([]);
    movePathMock.mockResolvedValue({});
  });

  it("normalizes the initial path when opening", () => {
    const { dialog } = setup();
    dialog.openMoveDialog([entry("docs/a.txt")], "  /docs/nested/ ");

    expect(dialog.showMoveDialog.value).toBe(true);
    expect(dialog.moveDialogInitialPath.value).toBe("docs/nested");
    expect(dialog.moveDialogItems.value).toHaveLength(1);
  });

  it("opens for a single entry starting from its parent", () => {
    const { dialog } = setup();
    dialog.openMoveForEntry(entry("docs/nested/a.txt"));

    expect(dialog.moveDialogInitialPath.value).toBe("docs/nested");
    expect(dialog.moveDialogItems.value[0].path).toBe("docs/nested/a.txt");
  });

  it("does not close while a move is submitting", () => {
    const { dialog } = setup();
    dialog.openMoveDialog([entry("docs/a.txt")], "docs");
    dialog.moveDialogSubmitting.value = true;

    dialog.closeMoveDialog();
    expect(dialog.showMoveDialog.value).toBe(true);
  });

  it("moves every selected entry and refreshes", async () => {
    const harness = setup();
    harness.dialog.openMoveDialog(
      [entry("docs/a.txt"), entry("docs/b.txt")],
      "photos",
    );

    await harness.dialog.submitMoveDialog("photos");

    expect(movePathMock).toHaveBeenCalledTimes(2);
    expect(movePathMock).toHaveBeenCalledWith(
      "docs/a.txt",
      "photos/a.txt",
      expect.stringContaining("移动文件"),
    );
    expect(harness.replaceSelectedPath).toHaveBeenCalledWith(
      "docs/a.txt",
      "photos/a.txt",
    );
    expect(harness.clearSelection).toHaveBeenCalledTimes(1);
    expect(harness.refreshAfterMutation).toHaveBeenCalledTimes(1);
    expect(harness.dialog.showMoveDialog.value).toBe(false);
    expect(harness.dialog.moveDialogSubmitting.value).toBe(false);
  });

  it("keeps the dialog open and reports collisions", async () => {
    const harness = setup();
    getFilesMock.mockResolvedValueOnce([{ path: "photos/a.txt" }]);
    harness.dialog.openMoveDialog([entry("docs/a.txt")], "photos");

    await harness.dialog.submitMoveDialog("photos");

    expect(movePathMock).not.toHaveBeenCalled();
    expect(harness.dialog.showMoveDialog.value).toBe(true);
    expect(harness.dialog.moveDialogSubmitting.value).toBe(false);
    expect(harness.refreshAfterMutation).not.toHaveBeenCalled();
  });

  it("clears the highlight when the moved entry leaves the current directory", async () => {
    const harness = setup();
    harness.setActivePath("docs/a.txt");
    harness.dialog.openMoveDialog([entry("docs/a.txt")], "photos");

    await harness.dialog.submitMoveDialog("photos");

    expect(harness.setActivePath).toHaveBeenCalledWith("");
  });

  it("moves a dragged entry into a directory without opening the dialog", async () => {
    const harness = setup();

    const ok = await harness.dialog.moveEntryToDirectory(
      entry("docs/a.txt"),
      "photos",
    );

    expect(ok).toBe(true);
    expect(movePathMock).toHaveBeenCalledWith(
      "docs/a.txt",
      "photos/a.txt",
      expect.stringContaining("移动文件"),
    );
    expect(harness.dialog.showMoveDialog.value).toBe(false);
    expect(harness.refreshAfterMutation).toHaveBeenCalledTimes(1);
  });

  it("refuses to move a directory into itself", async () => {
    const harness = setup();

    const ok = await harness.dialog.moveEntryToDirectory(
      entry("docs", "directory"),
      "docs/nested",
    );

    expect(ok).toBe(false);
    expect(movePathMock).not.toHaveBeenCalled();
    expect(harness.refreshAfterMutation).not.toHaveBeenCalled();
  });
});
