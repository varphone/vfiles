import { beforeEach, describe, expect, it, vi } from "vitest";
import { createPinia, setActivePinia } from "pinia";
import { ref } from "vue";
import { useFileSelection } from "../src/composables/useFileSelection";
import type { FileInfo } from "../src/types";

const { deleteFileMock, downloadFileMock, downloadFolderMock, confirmMock } =
  vi.hoisted(() => ({
    deleteFileMock: vi.fn(async () => ({})),
    downloadFileMock: vi.fn(),
    downloadFolderMock: vi.fn(),
    confirmMock: vi.fn(async () => true),
  }));

vi.mock("../src/services/files.service", () => ({
  filesService: {
    deleteFile: deleteFileMock,
    downloadFile: downloadFileMock,
    downloadFolder: downloadFolderMock,
  },
}));

vi.mock("../src/composables/dialog", () => ({
  confirmDialog: confirmMock,
}));

function file(name: string): FileInfo {
  return {
    id: name,
    name,
    path: name,
    kind: "file",
    created_at: "2026-01-01T00:00:00.000Z",
  };
}

function setup(
  files: FileInfo[] = [file("a.txt"), file("b.txt"), file("c.txt")],
) {
  const searchActive = ref(false);
  const visible = ref(files);
  const pool = ref(files);
  const setActivePath = vi.fn();
  const currentPath = ref("docs");
  const browseCommit = ref<string | undefined>(undefined);
  const refresh = vi.fn(async () => {});
  const doSearch = vi.fn(async () => {});
  const openMoveDialog = vi.fn();
  const renameEntry = vi.fn(async () => {});

  const selection = useFileSelection({
    searchActive,
    getVisibleItems: () => visible.value,
    getSelectionPool: () => pool.value,
    setActivePath,
    currentPath,
    browseCommit,
    refresh,
    doSearch,
    openMoveDialog,
    renameEntry,
  });

  return {
    selection,
    searchActive,
    setActivePath,
    refresh,
    doSearch,
    openMoveDialog,
    renameEntry,
  };
}

describe("useFileSelection", () => {
  beforeEach(() => {
    setActivePinia(createPinia());
    deleteFileMock.mockReset();
    downloadFileMock.mockReset();
    downloadFolderMock.mockReset();
    confirmMock.mockReset();
    deleteFileMock.mockResolvedValue({});
    confirmMock.mockResolvedValue(true);
  });

  it("toggles individual selection and hides it when leaving batch mode", () => {
    const { selection, setActivePath } = setup();
    const a = file("a.txt");

    selection.toggleSelect(a);
    expect(selection.selectedCount.value).toBe(1);
    expect(selection.batchMode.value).toBe(true); // r142 ✓ 勾选即批量（源级语义）
    expect(setActivePath).toHaveBeenCalledWith("a.txt");

    selection.toggleBatchMode();
    expect(selection.batchMode.value).toBe(false);
    expect(selection.selectedCount.value).toBe(0);
  });

  it("selects a range with shift after a ctrl click", () => {
    const [a, , c] = [file("a.txt"), file("b.txt"), file("c.txt")];
    const { selection } = setup([a, file("b.txt"), c]);

    selection.handleModifierSelect({ file: a, shift: false, meta: true });
    selection.handleModifierSelect({ file: c, shift: true, meta: false });

    expect(selection.batchMode.value).toBe(true);
    expect(selection.selectedCount.value).toBe(3);
  });

  it("selects all visible entries then clears on a second call", () => {
    const { selection } = setup();
    selection.batchMode.value = true;

    selection.toggleSelectAll();
    expect(selection.selectedCount.value).toBe(3);

    selection.toggleSelectAll();
    expect(selection.selectedCount.value).toBe(0);
  });

  it("deletes every selected entry and refreshes the view", async () => {
    const harness = setup();
    harness.searchActive.value = true;
    harness.selection.toggleSelect(file("a.txt"));
    harness.selection.toggleSelect(file("b.txt"));

    await harness.selection.batchDelete();

    expect(deleteFileMock).toHaveBeenCalledTimes(2);
    expect(harness.refresh).toHaveBeenCalledTimes(1);
    expect(harness.doSearch).toHaveBeenCalledWith(false);
    expect(harness.selection.selectedCount.value).toBe(0);
  });

  it("aborts the batch delete when the confirmation is dismissed", async () => {
    const harness = setup();
    confirmMock.mockResolvedValueOnce(false);
    harness.selection.toggleSelect(file("a.txt"));

    await harness.selection.batchDelete();

    expect(deleteFileMock).not.toHaveBeenCalled();
  });

  it("opens the move dialog with the current directory", () => {
    const harness = setup();
    harness.selection.toggleSelect(file("a.txt"));

    harness.selection.batchMove();

    expect(harness.openMoveDialog).toHaveBeenCalledWith(
      [expect.objectContaining({ path: "a.txt" })],
      "docs",
    );
  });

  it("renames only when exactly one entry is selected", async () => {
    const harness = setup();
    harness.selection.toggleSelect(file("a.txt"));
    harness.selection.toggleSelect(file("b.txt"));

    await harness.selection.renameSelected();
    expect(harness.renameEntry).not.toHaveBeenCalled();

    harness.selection.toggleSelect(file("b.txt"));
    await harness.selection.renameSelected();
    expect(harness.renameEntry).toHaveBeenCalledWith(
      expect.objectContaining({ path: "a.txt" }),
    );
  });
});
