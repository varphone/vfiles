import { fireEvent, render, screen, waitFor } from "@testing-library/vue";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { createPinia, setActivePinia } from "pinia";
import AccessTokens from "../src/views/AccessTokens.vue";

const {
  listTokensMock,
  expiryOptionsMock,
  createTokenMock,
  revokeTokenMock,
  confirmMock,
  copyMock,
  pushMock,
} = vi.hoisted(() => ({
  listTokensMock: vi.fn(),
  expiryOptionsMock: vi.fn(),
  createTokenMock: vi.fn(),
  revokeTokenMock: vi.fn(),
  confirmMock: vi.fn(),
  copyMock: vi.fn(),
  pushMock: vi.fn(),
}));

vi.mock("../src/services/files.service", () => ({
  filesService: {
    listAccessTokens: listTokensMock,
    listTokenExpiryOptions: expiryOptionsMock,
    createAccessToken: createTokenMock,
    revokeAccessToken: revokeTokenMock,
  },
}));

vi.mock("../src/composables/dialog", () => ({ confirmDialog: confirmMock }));
vi.mock("../src/utils/clipboard", () => ({ copyText: copyMock }));
vi.mock("vue-router", () => ({ useRouter: () => ({ push: pushMock }) }));

const token = {
  id: "t-1",
  name: "CI 构建",
  token_prefix: "vfat_1a2b3c4d",
  scopes: "full",
  created_at: new Date().toISOString(),
  expires_at: null,
  last_used_at: null,
  revoked_at: null,
  active: true,
};

function renderPage() {
  const pinia = createPinia();
  setActivePinia(pinia);
  return render(AccessTokens as any, { global: { plugins: [pinia] } });
}

describe("AccessTokens.vue", () => {
  beforeEach(() => {
    listTokensMock.mockReset();
    listTokensMock.mockResolvedValue([token]);
    expiryOptionsMock.mockReset();
    expiryOptionsMock.mockResolvedValue([
      { days: 0, label: "永久" },
      { days: 90, label: "3 个月" },
    ]);
    createTokenMock.mockReset();
    revokeTokenMock.mockReset();
    confirmMock.mockReset();
    confirmMock.mockResolvedValue(true);
    copyMock.mockReset();
    copyMock.mockResolvedValue(true);
    pushMock.mockReset();
  });

  it("lists tokens with prefix, status and usage hint", async () => {
    const { container } = renderPage();

    await waitFor(() =>
      expect(screen.getByText("CI 构建")).toBeInTheDocument(),
    );
    expect(container.querySelector(".tokens-prefix")?.textContent).toContain(
      "vfat_1a2b3c4d",
    );
    expect(screen.getByText("从未使用")).toBeInTheDocument();
    expect(screen.getByText("有效")).toBeInTheDocument();
    // 明示令牌只能在创建时看到一次
    expect(screen.getByText(/令牌只在创建时显示一次/)).toBeInTheDocument();
    // 用法示例包含 Bearer 头
    expect(
      container.querySelector(".tokens-usage-code")?.textContent,
    ).toContain("Authorization: Bearer");
  });

  it("shows the empty state with a create action", async () => {
    listTokensMock.mockResolvedValue([]);
    renderPage();

    await waitFor(() =>
      expect(screen.getByText("还没有访问令牌")).toBeInTheDocument(),
    );
    // 页头与空态各有一个入口
    expect(screen.getAllByText("新建令牌").length).toBeGreaterThanOrEqual(1);
  });

  it("creates a token, shows the plaintext once and copies it", async () => {
    createTokenMock.mockResolvedValue({
      token,
      plaintext: "vfat_abcdef0123456789",
    });
    const { container } = renderPage();
    await waitFor(() =>
      expect(screen.getByText("CI 构建")).toBeInTheDocument(),
    );

    await fireEvent.click(screen.getByText("新建令牌"));
    await fireEvent.update(document.querySelector("#token-name")!, "备份脚本");
    await fireEvent.click(screen.getByText("创建"));

    await waitFor(() => expect(createTokenMock).toHaveBeenCalled());
    expect(createTokenMock).toHaveBeenCalledWith("备份脚本", 0);

    // 明文对话框出现，输入框带完整令牌
    await waitFor(() =>
      expect(
        (document.querySelector(".tokens-created-input") as HTMLInputElement)
          ?.value,
      ).toBe("vfat_abcdef0123456789"),
    );
    await fireEvent.click(screen.getByText("复制"));
    await waitFor(() =>
      expect(copyMock).toHaveBeenCalledWith("vfat_abcdef0123456789"),
    );
    await waitFor(() => expect(screen.getByText("已复制")).toBeInTheDocument());

    // 关闭后对话框不再激活（Modal 的插槽内容常驻，用 is-active 判断可见性）
    await fireEvent.click(screen.getByText("我已保存"));
    await waitFor(() =>
      expect(container.querySelector(".modal.is-active")).toBeNull(),
    );
  });

  it("passes the selected expiry when creating", async () => {
    createTokenMock.mockResolvedValue({ token, plaintext: "vfat_x" });
    renderPage();
    await waitFor(() =>
      expect(screen.getByText("CI 构建")).toBeInTheDocument(),
    );

    await fireEvent.click(screen.getByText("新建令牌"));
    await fireEvent.update(document.querySelector("#token-name")!, "临时");
    await fireEvent.update(document.querySelector("#token-expiry")!, "90");
    await fireEvent.click(screen.getByText("创建"));

    await waitFor(() =>
      expect(createTokenMock).toHaveBeenCalledWith("临时", 90),
    );
  });

  it("revokes after confirmation and reports failures", async () => {
    renderPage();
    await waitFor(() =>
      expect(screen.getByText("CI 构建")).toBeInTheDocument(),
    );

    await fireEvent.click(screen.getByText("撤销"));
    await waitFor(() => expect(confirmMock).toHaveBeenCalled());
    await waitFor(() => expect(revokeTokenMock).toHaveBeenCalledWith("t-1"));
    expect(listTokensMock).toHaveBeenCalledTimes(2);

    // 取消失败时提示错误且不刷新
    confirmMock.mockResolvedValue(false);
    revokeTokenMock.mockClear();
    await fireEvent.click(screen.getByText("撤销"));
    await waitFor(() => expect(confirmMock).toHaveBeenCalledTimes(2));
    expect(revokeTokenMock).not.toHaveBeenCalled();
  });

  it("surfaces a load failure with retry", async () => {
    listTokensMock.mockRejectedValue(new Error("网络错误"));
    renderPage();

    await waitFor(() =>
      expect(screen.getByText("加载失败")).toBeInTheDocument(),
    );
    listTokensMock.mockResolvedValue([token]);
    await fireEvent.click(screen.getByText("重试"));
    await waitFor(() =>
      expect(screen.getByText("CI 构建")).toBeInTheDocument(),
    );
  });

  it("marks revoked and expired tokens", async () => {
    listTokensMock.mockResolvedValue([
      { ...token, revoked_at: new Date().toISOString(), active: false },
      {
        ...token,
        id: "t-2",
        name: "过期的",
        active: false,
        expires_at: new Date(Date.now() - 86400000).toISOString(),
      },
    ]);
    const { container } = renderPage();

    // 状态徽标「已撤销」与操作列的「已撤销」文案都会出现
    await waitFor(() =>
      expect(screen.getAllByText("已撤销").length).toBeGreaterThanOrEqual(1),
    );
    expect(screen.getByText("已过期")).toBeInTheDocument();
    expect(
      container.querySelectorAll(".tokens-status.is-revoked"),
    ).toHaveLength(1);
    expect(
      container.querySelectorAll(".tokens-status.is-expired"),
    ).toHaveLength(1);
    // 已撤销的令牌不再显示撤销按钮，只剩一个
    expect(container.querySelectorAll(".tokens-revoke")).toHaveLength(1);
  });
});
