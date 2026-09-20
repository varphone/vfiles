import { describe, expect, it } from "vitest";
import {
  API_ERROR_MESSAGES,
  extractErrorPayload,
  localizeApiError,
} from "../src/utils/apiErrors";

describe("localizeApiError", () => {
  it("renders a Chinese message for known codes", () => {
    expect(localizeApiError({ code: "ENTRY_NOT_FOUND" }, "兜底")).toBe(
      API_ERROR_MESSAGES.ENTRY_NOT_FOUND,
    );
    expect(localizeApiError({ code: "rate_limited" }, "兜底")).toBe(
      API_ERROR_MESSAGES.RATE_LIMITED,
    );
  });

  it("appends the conflicting path when the server provides it", () => {
    expect(
      localizeApiError(
        {
          code: "PATH_CONFLICT",
          message: "Path conflict: Path already exists: docs/a.txt",
          details: { path: "docs/a.txt" },
        },
        "兜底",
      ),
    ).toBe("目标路径已被占用：docs/a.txt");
  });

  it("renders the size limit for oversized uploads", () => {
    expect(
      localizeApiError(
        {
          code: "FILE_TOO_LARGE",
          message: "File too large. Maximum size is 104857600 bytes",
          details: { limit_bytes: 104857600, size_bytes: 200000000 },
        },
        "兜底",
      ),
    ).toBe("文件过大，已超过上限（最大 100 MB）");
  });

  it("names the offending field for validation failures", () => {
    expect(
      localizeApiError(
        {
          code: "VALIDATION_FAILED",
          message: "Validation failed for password: too short",
          details: { field: "password", reason: "too short" },
        },
        "兜底",
      ),
    ).toBe("输入内容不合法：密码");

    // 未知字段直接回退字段名，不展示英文原因
    expect(
      localizeApiError(
        { code: "VALIDATION_FAILED", details: { field: "nickname" } },
        "兜底",
      ),
    ).toBe("输入内容不合法：nickname");
  });

  it("keeps the server message for unknown codes", () => {
    expect(
      localizeApiError(
        { code: "SOMETHING_NEW", message: "Custom server hint" },
        "兜底",
      ),
    ).toBe("Custom server hint");
  });

  it("falls back when the payload is missing or empty", () => {
    expect(localizeApiError(null, "网络错误")).toBe("网络错误");
    expect(localizeApiError({}, "网络错误")).toBe("网络错误");
    expect(localizeApiError({ message: "   " }, "网络错误")).toBe("网络错误");
  });
});

describe("extractErrorPayload", () => {
  it("reads the standard error body and the data-wrapped form", () => {
    expect(
      extractErrorPayload({ code: "FORBIDDEN", message: "Access denied" }),
    ).toEqual({ code: "FORBIDDEN", message: "Access denied" });
    expect(
      extractErrorPayload({
        success: false,
        data: { code: "NOT_FOUND", message: "missing" },
      }),
    ).toEqual({ code: "NOT_FOUND", message: "missing" });
  });

  it("supports the legacy error field and ignores unrelated payloads", () => {
    expect(extractErrorPayload({ error: "boom" })).toEqual({ message: "boom" });
    expect(extractErrorPayload("plain text")).toBeNull();
    expect(extractErrorPayload({ success: true })).toBeNull();
  });
});
