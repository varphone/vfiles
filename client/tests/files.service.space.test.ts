import { afterEach, describe, expect, it, vi } from "vitest";
import { filesService } from "../src/services/files.service";

afterEach(() => {
  vi.restoreAllMocks();
});

describe("filesService.getSpaceAnalysis", () => {
  it("passes the directory path and normalizes the response", async () => {
    const { apiService } = await import("../src/services/api.service");
    const getMock = vi.spyOn(apiService, "get").mockResolvedValue({
      success: true,
      data: {
        path: "docs",
        total_bytes: 120,
        file_count: 3,
        directory_count: 1,
        direct_file_bytes: 20,
        direct_file_count: 1,
        folders: [
          {
            path: "docs/nested",
            name: "nested",
            bytes: 100,
            file_count: 2,
            directory_count: 1,
          },
        ],
        files: [
          { path: "docs/a.txt", name: "a.txt", bytes: 20, mime_type: null },
        ],
        largest_files: [
          { path: "docs/nested/b.bin", name: "b.bin", bytes: 80 },
        ],
      },
    });

    const analysis = await filesService.getSpaceAnalysis("docs");

    expect(getMock).toHaveBeenCalledWith("/files/space", { path: "docs" });
    expect(analysis.total_bytes).toBe(120);
    expect(analysis.folders[0]).toMatchObject({
      path: "docs/nested",
      bytes: 100,
      file_count: 2,
    });
    expect(analysis.files[0].mime_type).toBeNull();
    expect(analysis.largest_files[0].bytes).toBe(80);
  });

  it("requests the namespace root when no path is given", async () => {
    const { apiService } = await import("../src/services/api.service");
    const getMock = vi.spyOn(apiService, "get").mockResolvedValue({
      success: true,
      data: {
        path: "",
        total_bytes: 0,
        file_count: 0,
        directory_count: 0,
        direct_file_bytes: 0,
        direct_file_count: 0,
        folders: [],
        files: [],
        largest_files: [],
      },
    });

    await filesService.getSpaceAnalysis();

    expect(getMock).toHaveBeenCalledWith("/files/space", {});
  });
});
