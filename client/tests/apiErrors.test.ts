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
