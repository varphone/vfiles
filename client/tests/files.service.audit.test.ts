import { describe, expect, it, vi } from "vitest";

const { getMock } = vi.hoisted(() => ({ getMock: vi.fn() }));

vi.mock("../src/services/api.service", () => ({
  apiService: { get: getMock },
  ApiError: class ApiError extends Error {},
}));

import { filesService } from "../src/services/files.service";

describe("filesService share endpoints", () => {
  it("stops a share via /share/shares/{code}", async () => {
    const { apiService } = await import("../src/services/api.service");
    const deleteMock = vi.fn(async () => ({}));
    (apiService as any).delete = deleteMock;

    const { filesService: service } =
      await import("../src/services/files.service");
    await service.disableShare("abc123");

    // 写成 /shares/{code} 会命中前端回退并返回 200，服务端不会真正停用链接
    expect(deleteMock).toHaveBeenCalledWith("/share/shares/abc123");
  });
});

describe("filesService.listAuditLogs", () => {
  it("passes filters as query params (not an axios config object)", async () => {
    getMock.mockResolvedValue({
      items: [],
      total: 0,
      limit: 50,
      offset: 50,
    });

    await filesService.listAuditLogs({
      keyword: "alice",
      action: "file.upload",
      result: "failure",
      limit: 50,
      offset: 50,
    });

    // 第二参数必须是扁平的查询参数：写成 { params: {...} } 时服务端会忽略全部筛选
    expect(getMock).toHaveBeenCalledWith("/audit/logs", {
      keyword: "alice",
      action: "file.upload",
      result: "failure",
      since: undefined,
      until: undefined,
      limit: 50,
      offset: 50,
    });
  });

  it("normalises a missing payload", async () => {
    getMock.mockResolvedValue(undefined);

    await expect(filesService.listAuditLogs()).resolves.toEqual({
      items: [],
      total: 0,
      limit: 0,
      offset: 0,
    });
  });
});
