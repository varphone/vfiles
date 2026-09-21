import { fireEvent, render, screen, waitFor } from "@testing-library/vue";
import { beforeEach, describe, expect, it, vi } from "vitest";
import ShareDialog from "../src/components/common/ShareDialog.vue";
import type { FileInfo } from "../src/types";

const { createShareMock, disableShareMock, copyTextMock } = vi.hoisted(() => ({
  createShareMock: vi.fn(),
  disableShareMock: vi.fn(async () => undefined),
  copyTextMock: vi.fn(async () => true),
}));

vi.mock("../src/services/files.service", () => ({
  SEARCH_PAGE_SIZE: 100,
  filesService: {
    createShareLink: createShareMock,
    disableShare: disableShareMock,
  },
}));

vi.mock("../src/utils/clipboard", () => ({ copyText: copyTextMock }));

const file: FileInfo = {
  id: "a",
  name: "报告.txt",
  path: "文档/报告.txt",
  kind: "file",
  size_bytes: 10,
  created_at: "2026-09-20T10:00:00.000Z",
  updated_at: "2026-09-20T10:00:00.000Z",
};

function renderDialog(overrides: Record<string, unknown> = {}) {
  return render(ShareDialog as any, {
    props: {
      isActive: true,
      filePath: "文档/报告.txt",
      file,
      ...overrides,
    },
  });
}

describe("ShareDialog.vue", () => {
  beforeEach(() => {
    createShareMock.mockReset();
    createShareMock.mockResolvedValue({
      code: "abc123",
      url: "https://files.example.com/s/abc123",
      expiresIn: 604800,
      expiresAt: "2026-10-01T10:00:00.000Z",
    });
    disableShareMock.mockClear();
    copyTextMock.mockClear();
  });

  it("shows the shared item instead of an editable path field", () => {
    const { container } = renderDialog();

    expect(screen.getByText("报告.txt")).toBeInTheDocument();
    expect(screen.getByText("/文档/报告.txt")).toBeInTheDocument();
    expect(screen.queryByText("文件路径")).toBeNull();
    expect(container.querySelector(".share-item-icon svg")).not.toBeNull();
    expect(screen.getByRole("button", { name: /生成链接/ })).toBeEnabled();
  });

  it("creates a link and offers copy, open and stop", async () => {
    renderDialog();

    await fireEvent.click(screen.getByRole("button", { name: /生成链接/ }));

    const input = await screen.findByDisplayValue(
      "https://files.example.com/s/abc123",
    );
    expect(createShareMock).toHaveBeenCalledWith("文档/报告.txt", {
      commit: undefined,
      ttl: 604800,
    });
    expect(input).toHaveAttribute("readonly");
    expect(screen.getByText(/有效期至/)).toBeInTheDocument();
    expect(
      screen.getByRole("button", { name: /打开链接/ }),
    ).toBeInTheDocument();
    expect(
      screen.getByRole("button", { name: /停止分享/ }),
    ).toBeInTheDocument();

    await fireEvent.click(screen.getByRole("button", { name: /复制/ }));
    await waitFor(() =>
      expect(copyTextMock).toHaveBeenCalledWith(
        "https://files.example.com/s/abc123",
      ),
    );
    expect(await screen.findByText(/链接已复制到剪贴板/)).toBeInTheDocument();
  });

  it("stops sharing and returns to the form", async () => {
    renderDialog();
    await fireEvent.click(screen.getByRole("button", { name: /生成链接/ }));
    await screen.findByDisplayValue("https://files.example.com/s/abc123");

    await fireEvent.click(screen.getByRole("button", { name: /停止分享/ }));

    await waitFor(() =>
      expect(disableShareMock).toHaveBeenCalledWith("abc123"),
    );
    expect(
      await screen.findByRole("button", { name: /生成链接/ }),
    ).toBeInTheDocument();
    expect(
      screen.queryByDisplayValue("https://files.example.com/s/abc123"),
    ).toBeNull();
  });

  it("reports a readable error when link creation fails", async () => {
    createShareMock.mockRejectedValue(new Error("没有权限执行该操作"));
    renderDialog();

    await fireEvent.click(screen.getByRole("button", { name: /生成链接/ }));

    await waitFor(() =>
      expect(screen.getByRole("alert").textContent).toContain(
        "没有权限执行该操作",
      ),
    );
  });

  it("offers 1 年 and 永久 expiry options and marks permanent links", async () => {
    // 永久分享没有过期时间（真实服务端在 ttl=0 时返回空字符串）
    createShareMock.mockResolvedValueOnce({
      code: "perm1",
      url: "https://files.example.com/s/perm1",
      expiresIn: 0,
      expiresAt: "",
    });
    const { container } = renderDialog();

    const options = Array.from(
      container.querySelectorAll<HTMLOptionElement>("#share-ttl option"),
    ).map((option) => option.textContent?.trim());
    expect(options).toEqual([
      "1 小时",
      "1 天",
      "7 天",
      "30 天",
      "1 年",
      "永久",
    ]);

    // 选择永久时提示语随之变化
    await fireEvent.update(
      container.querySelector<HTMLSelectElement>("#share-ttl")!,
      "0",
    );
    expect(container.querySelector(".share-hint")?.textContent).toContain(
      "永久链接不会自动失效",
    );

    await fireEvent.click(screen.getByRole("button", { name: /生成链接/ }));
    await waitFor(() =>
      expect(container.querySelector(".share-chip")?.textContent).toContain(
        "永久有效",
      ),
    );
    // 永久分享不发送有效期（服务端视为不过期）
    expect(createShareMock).toHaveBeenCalledWith("文档/报告.txt", {
      commit: undefined,
      ttl: 0,
    });
  });

  it("sends a one-year expiry when 1 年 is selected", async () => {
    const { container } = renderDialog();

    await fireEvent.update(
      container.querySelector<HTMLSelectElement>("#share-ttl")!,
      "31536000",
    );
    await fireEvent.click(screen.getByRole("button", { name: /生成链接/ }));

    await waitFor(() =>
      expect(createShareMock).toHaveBeenCalledWith("文档/报告.txt", {
        commit: undefined,
        ttl: 31536000,
      }),
    );
  });

  it("resets the generated link when the dialog closes", async () => {
    const { rerender } = renderDialog();
    await fireEvent.click(screen.getByRole("button", { name: /生成链接/ }));
    await screen.findByDisplayValue("https://files.example.com/s/abc123");

    await rerender({ isActive: false });
    await rerender({ isActive: true });

    expect(
      screen.queryByDisplayValue("https://files.example.com/s/abc123"),
    ).toBeNull();
    expect(
      screen.getByRole("button", { name: /生成链接/ }),
    ).toBeInTheDocument();
  });
});
