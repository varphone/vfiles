import { beforeEach, describe, expect, it, vi } from "vitest";

const { getMock, postMock } = vi.hoisted(() => ({
  getMock: vi.fn(),
  postMock: vi.fn(),
}));

vi.mock("../src/services/api.service", () => ({
  apiService: {
    get: getMock,
    post: postMock,
  },
}));

vi.mock("../src/services/fetch-retry", () => ({
  fetchWithRetry: vi.fn(),
}));

import { filesService } from "../src/services/files.service";

describe("filesService movement operations", () => {
  beforeEach(() => {
    getMock.mockReset();
    postMock.mockReset();
  });

  it("requests bounded pages of direct child directories", async () => {
    getMock.mockResolvedValue({
      items: [
        { id: "1", name: "nested", path: "docs/nested", kind: "directory" },
      ],
      total: 201,
      limit: 100,
      offset: 100,
      has_more: true,
    });

    const page = await filesService.getDirectoriesPage("docs/nested space", {
      limit: 100,
      offset: 100,
    });

    expect(getMock).toHaveBeenCalledWith(
      "/files/directories/docs%2Fnested%20space?limit=100&offset=100",
    );
    expect(page).toEqual({
      items: [
        { id: "1", name: "nested", path: "docs/nested", kind: "directory" },
      ],
      total: 201,
      limit: 100,
      offset: 100,
      has_more: true,
    });
  });

  it("sends one atomic batch move request", async () => {
    postMock.mockResolvedValue(undefined);

    await filesService.movePaths(["docs/a.txt", "docs/b.txt"], "archive");

    expect(postMock).toHaveBeenCalledWith("/files/move/batch", {
      sources: ["docs/a.txt", "docs/b.txt"],
      destination: "archive",
      message: undefined,
    });
  });

  it("rejects a batch above the server limit before sending a request", async () => {
    const sources = Array.from(
      { length: 501 },
      (_, index) => `file-${index}.txt`,
    );

    await expect(filesService.movePaths(sources, "archive")).rejects.toThrow(
      "一次最多移动 500 个项目",
    );
    expect(postMock).not.toHaveBeenCalled();
  });
});
