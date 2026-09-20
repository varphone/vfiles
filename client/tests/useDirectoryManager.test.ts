import { beforeEach, describe, expect, it, vi } from "vitest";
import { createPinia, setActivePinia } from "pinia";
import { ref, nextTick } from "vue";
import { useDirectoryManager } from "../src/composables/useDirectoryManager";

const { createDirectoryMock, movePathMock, deleteFileMock, promptDialogMock } =
  vi.hoisted(() => ({
    createDirectoryMock: vi.fn(async () => ({})),
    movePathMock: vi.fn(async () => ({})),
    deleteFileMock: vi.fn(async () => ({})),
    promptDialogMock: vi.fn(async () => null as string | null),
  }));

vi.mock("../src/services/files.service", () => ({
  filesService: {
    createDirectory: createDirectoryMock,
    movePath: movePathMock,
    deleteFile: deleteFileMock,
  },
}));

vi.mock("../src/composables/dialog", () => ({
  promptDialog: promptDialogMock,
}));

interface Harness {
  manager: ReturnType<typeof useDirectoryManager>;
  currentPath: ReturnType<typeof ref<string>>;
  searchActive: ReturnType<typeof ref<boolean>>;
  navigateTo: ReturnType<typeof vi.fn>;
  refresh: ReturnType<typeof vi.fn>;
  doSearch: ReturnType<typeof vi.fn>;
  setActivePath: ReturnType<typeof vi.fn>;
  clearSearch: ReturnType<typeof vi.fn>;
}

function setup(initialPath = "docs"): Harness {
  const currentPath = ref(initialPath);
  const searchActive = ref(false);
  const navigateTo = vi.fn();
  const refresh = vi.fn(async () => {});
  const doSearch = vi.fn(async () => {});
  const setActivePath = vi.fn();
  const clearSearch = vi.fn();

  const manager = useDirectoryManager({
    currentPath,
    searchActive,
    clearSearch,
    navigateTo,
    refresh,
    doSearch,
    setActivePath,
  });

  return {
    manager,
    currentPath,
    searchActive,
    navigateTo,
    refresh,
    doSearch,
    setActivePath,
    clearSearch,
  };
}

describe("useDirectoryManager", () => {
  beforeEach(() => {
    setActivePinia(createPinia());
    createDirectoryMock.mockReset();
    movePathMock.mockReset();
    deleteFileMock.mockReset();
    promptDialogMock.mockReset();
    createDirectoryMock.mockResolvedValue({});
    movePathMock.mockResolvedValue({});
    deleteFileMock.mockResolvedValue({});
    promptDialogMock.mockResolvedValue(null);
  });

  it("tracks the current directory name", async () => {
    const { manager, currentPath } = setup("docs/nested");
    expect(manager.currentDirName.value).toBe("nested");

    currentPath.value = "docs";
    await nextTick();
    expect(manager.renameDirName.value).toBe("docs");
  });

  it("creates a subdirectory and refreshes the view", async () => {
    const harness = setup("docs");
    harness.searchActive.value = true;
    harness.manager.newDirName.value = "reports";

    await harness.manager.createSubDir();

    expect(createDirectoryMock).toHaveBeenCalledWith(
      "docs/reports",
      expect.stringContaining("创建目录"),
    );
    expect(harness.manager.newDirName.value).toBe("");
    expect(harness.setActivePath).toHaveBeenCalledWith("docs/reports");
    expect(harness.refresh).toHaveBeenCalledTimes(1);
    expect(harness.doSearch).toHaveBeenCalledWith(false);
  });

  it("renames the current directory and navigates to the new path", async () => {
    const harness = setup("docs/nested");
    harness.manager.renameDirName.value = "renamed";

    await harness.manager.renameCurrentDir();

    expect(movePathMock).toHaveBeenCalledWith(
      "docs/nested",
      "docs/renamed",
      expect.stringContaining("重命名目录"),
    );
    expect(harness.navigateTo).toHaveBeenCalledWith("docs/renamed");
    expect(harness.manager.dirManagerOpen.value).toBe(false);
  });

  it("rejects an unchanged directory name", async () => {
    const harness = setup("docs/nested");
    harness.manager.renameDirName.value = "nested";

    await harness.manager.renameCurrentDir();

    expect(movePathMock).not.toHaveBeenCalled();
  });

  it("requires the typed name to match before deleting", async () => {
    const harness = setup("docs/nested");
    promptDialogMock.mockResolvedValueOnce("wrong-name");

    await harness.manager.deleteCurrentDir();

    expect(deleteFileMock).not.toHaveBeenCalled();

    promptDialogMock.mockResolvedValueOnce("nested");
    await harness.manager.deleteCurrentDir();

    expect(deleteFileMock).toHaveBeenCalledWith(
      "docs/nested",
      expect.stringContaining("删除目录"),
    );
    expect(harness.navigateTo).toHaveBeenCalledWith("docs");
  });

  it("navigates to the parent when creating a directory elsewhere", async () => {
    const harness = setup("docs");
    promptDialogMock.mockResolvedValueOnce("child");

    await harness.manager.promptCreateDirectory("photos");

    expect(createDirectoryMock).toHaveBeenCalledWith(
      "photos/child",
      expect.any(String),
    );
    expect(harness.navigateTo).toHaveBeenCalledWith("photos");
    expect(harness.refresh).not.toHaveBeenCalled();
  });
});
