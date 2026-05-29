import { fireEvent, waitFor } from "@testing-library/vue";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { renderWithProviders } from "./renderWithProviders";
import FileBrowser from "../src/components/file-browser/FileBrowser.vue";

const { getFilesMock, searchFilesMock, deleteFileMock } = vi.hoisted(() => ({
  getFilesMock: vi.fn(async () => []),
  searchFilesMock: vi.fn(async () => []),
  deleteFileMock: vi.fn(async () => ({ success: true })),
}));

vi.mock("../src/services/files.service", () => ({
  filesService: {
    getFiles: getFilesMock,
    searchFiles: searchFilesMock,
    deleteFile: deleteFileMock,
  },
}));

function stubBrowserApis() {
  vi.stubGlobal("matchMedia", (q: string) => ({
    matches: false,
    media: q,
    onchange: null,
    addListener: () => {},
    removeListener: () => {},
    addEventListener: () => {},
    removeEventListener: () => {},
    dispatchEvent: () => false,
  }));
  vi.stubGlobal("confirm", vi.fn(() => true));
}

describe("FileBrowser.vue", () => {
  beforeEach(() => {
    getFilesMock.mockReset();
    searchFilesMock.mockReset();
    deleteFileMock.mockReset();
    getFilesMock.mockResolvedValue([]);
    searchFilesMock.mockResolvedValue([]);
    deleteFileMock.mockResolvedValue({ success: true });
    stubBrowserApis();
  });

  it("shows empty folder message when no files", async () => {
    const { findByText } = renderWithProviders(FileBrowser as any);
    await findByText("此文件夹为空");
  });

  it("refreshes search results after deleting a searched file", async () => {
    vi.stubGlobal("matchMedia", (q: string) => ({
      matches: true,
      media: q,
      onchange: null,
      addListener: () => {},
      removeListener: () => {},
      addEventListener: () => {},
      removeEventListener: () => {},
      dispatchEvent: () => false,
    }));

    searchFilesMock
      .mockResolvedValueOnce([
        {
          id: "file-1",
          name: "match.txt",
          path: "docs/match.txt",
          kind: "file",
          size_bytes: 12,
          created_at: "2026-04-09T00:00:00.000Z",
        },
      ])
      .mockResolvedValueOnce([]);

    const { findAllByText, findByPlaceholderText, findByText, queryByText } =
      renderWithProviders(FileBrowser as any);

    const input = await findByPlaceholderText("搜索文件名...");
    await fireEvent.update(input, "match");
    await fireEvent.keyUp(input, { key: "Enter", code: "Enter", charCode: 13 });

    await findByText("搜索结果：1 项（文件名）");
    const [fileName] = await findAllByText(
      (_, element) => element?.textContent === "match.txt",
    );
    await fireEvent.click(fileName.closest(".file-item") || fileName);
    await fireEvent.click(await findByText("删除"));

    await waitFor(() => {
      expect(deleteFileMock).toHaveBeenCalledWith("docs/match.txt", undefined);
      expect(searchFilesMock).toHaveBeenCalledTimes(2);
    });

    await findByText("没有找到匹配的文件");
    expect(queryByText("搜索结果：1 项（文件名）")).not.toBeInTheDocument();
  });
});
