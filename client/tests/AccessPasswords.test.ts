import { fireEvent, render, screen, waitFor, within } from "@testing-library/vue";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { createPinia, setActivePinia } from "pinia";
import { createMemoryHistory, createRouter } from "vue-router";
import AccessPasswords from "../src/views/AccessPasswords.vue";

const {
  listMock,
  createMock,
  updateMock,
  deleteMock,
  directoriesMock,
  confirmMock,
} = vi.hoisted(() => ({
  listMock: vi.fn(),
  createMock: vi.fn(),
  updateMock: vi.fn(),
  deleteMock: vi.fn(),
  directoriesMock: vi.fn(),
  confirmMock: vi.fn(),
}));

vi.mock("../src/services/auth.service", () => ({
  authService: {
    listLoginCredentials: listMock,
    createLoginCredential: createMock,
    updateLoginCredential: updateMock,
    deleteLoginCredential: deleteMock,
  },
}));

vi.mock("../src/services/files.service", () => ({
  filesService: {
    getDirectoriesPage: directoriesMock,
  },
}));

vi.mock("../src/composables/dialog", () => ({ confirmDialog: confirmMock }));

const credential = {
  id: "c-1",
  label: "read-docs",
  roots: ["docs", "images"],
  canWrite: false,
  createdAt: "2026-01-01T00:00:00Z",
  lastUsedAt: null,
};

const writableCredential = {
  id: "c-2",
  label: "write-docs",
  roots: ["docs"],
  canWrite: true,
  createdAt: "2026-01-02T00:00:00Z",
  lastUsedAt: "2026-01-03T00:00:00Z",
};

function stubCompactViewport() {
  vi.stubGlobal("matchMedia", (query: string) => ({
    matches: query.includes("max-width: 768px"),
    media: query,
    onchange: null,
    addEventListener: vi.fn(),
    removeEventListener: vi.fn(),
    addListener: vi.fn(),
    removeListener: vi.fn(),
    dispatchEvent: vi.fn(),
  }));
}

function renderPage() {
  const pinia = createPinia();
  setActivePinia(pinia);
  const router = createRouter({
    history: createMemoryHistory(),
    routes: [{ path: "/:pathMatch(.*)*", component: { render: () => null } }],
  });
  return render(AccessPasswords as any, {
    global: { plugins: [pinia, router] },
  });
}

describe("AccessPasswords.vue", () => {
  afterEach(() => vi.unstubAllGlobals());

  beforeEach(() => {
    listMock.mockReset();
    listMock.mockResolvedValue([credential]);
    createMock.mockReset();
    createMock.mockResolvedValue({ id: "c-new" });
    updateMock.mockReset();
    updateMock.mockResolvedValue({ updated: true });
    deleteMock.mockReset();
    deleteMock.mockResolvedValue(undefined);
    directoriesMock.mockReset();
    directoriesMock.mockResolvedValue({
      items: [
        {
          id: "d-1",
          name: "docs",
          path: "docs",
          kind: "directory",
          created_at: "2026-01-01T00:00:00Z",
        },
      ],
      total: 1,
      limit: 200,
      offset: 0,
      has_more: false,
    });
    confirmMock.mockReset();
    confirmMock.mockResolvedValue(true);
  });

  it("lists label, allowed directories, permission and last use", async () => {
    listMock.mockResolvedValue([credential, writableCredential]);
    renderPage();

    await waitFor(() =>
      expect(screen.getByText("read-docs")).toBeInTheDocument(),
    );
    expect(screen.getByText("write-docs")).toBeInTheDocument();
    expect(screen.getAllByText("docs").length).toBe(2);
    expect(screen.getByText("images")).toBeInTheDocument();
    expect(screen.getByText("只读")).toBeInTheDocument();
    expect(screen.getByText("可写")).toBeInTheDocument();
    expect(screen.getByText("从未使用")).toBeInTheDocument();
  });

  it("renders stacked cards instead of a table on narrow screens", async () => {
    stubCompactViewport();
    const { container } = renderPage();

    await waitFor(() =>
      expect(screen.getByText("read-docs")).toBeInTheDocument(),
    );
    expect(container.querySelector(".passwords-cards")).not.toBeNull();
    expect(container.querySelector(".passwords-table")).toBeNull();
    expect(container.querySelectorAll(".passwords-card")).toHaveLength(1);
  });

  it("shows a friendly empty state", async () => {
    listMock.mockResolvedValue([]);
    renderPage();

    await waitFor(() =>
      expect(screen.getByText("还没有访问密码")).toBeInTheDocument(),
    );
    expect(
      screen.getAllByRole("button", { name: "新建密码" }).length,
    ).toBeGreaterThanOrEqual(1);
  });

  it("creates a credential with a directory picked from the directory browser", async () => {
    listMock.mockResolvedValue([]);
    renderPage();

    await waitFor(() =>
      expect(screen.getByText("还没有访问密码")).toBeInTheDocument(),
    );
    await fireEvent.click(
      screen.getAllByRole("button", { name: "新建密码" })[0],
    );

    await fireEvent.update(screen.getByLabelText("备注"), "read-docs");
    await fireEvent.update(screen.getByLabelText("密码"), "secret123");
    await fireEvent.click(screen.getByRole("button", { name: "添加目录" }));

    await waitFor(() =>
      expect(screen.getByText("docs")).toBeInTheDocument(),
    );
    const row = screen.getByText("docs").closest(".directory-picker-row")!;
    expect(row).not.toBeNull();
    await fireEvent.click(within(row as HTMLElement).getByRole("button", { name: "选择" }));

    expect(document.querySelector(".password-root-chip")).not.toBeNull();
    await fireEvent.click(screen.getByRole("button", { name: "保存" }));

    await waitFor(() =>
      expect(createMock).toHaveBeenCalledWith({
        label: "read-docs",
        password: "secret123",
        roots: ["docs"],
        canWrite: false,
      }),
    );
  });

  it("edits without changing the password when the field is left blank", async () => {
    renderPage();
    await waitFor(() =>
      expect(screen.getByText("read-docs")).toBeInTheDocument(),
    );

    await fireEvent.click(screen.getByRole("button", { name: "编辑" }));
    await fireEvent.click(screen.getByRole("button", { name: "保存" }));

    await waitFor(() => expect(updateMock).toHaveBeenCalled());
    const [id, payload] = updateMock.mock.calls[0];
    expect(id).toBe("c-1");
    expect(payload).toEqual({
      label: "read-docs",
      roots: ["docs", "images"],
      canWrite: false,
    });
    expect(payload).not.toHaveProperty("password");
  });

  it("surfaces the backend validation message", async () => {
    listMock.mockResolvedValue([]);
    createMock.mockRejectedValue(
      new Error("密码长度需在 6-256 个字符之间"),
    );
    renderPage();

    await waitFor(() =>
      expect(screen.getByText("还没有访问密码")).toBeInTheDocument(),
    );
    await fireEvent.click(
      screen.getAllByRole("button", { name: "新建密码" })[0],
    );
    await fireEvent.update(screen.getByLabelText("备注"), "read-docs");
    await fireEvent.update(screen.getByLabelText("密码"), "long-enough");
    await fireEvent.click(screen.getByRole("button", { name: "添加目录" }));
    await waitFor(() =>
      expect(screen.getByText("docs")).toBeInTheDocument(),
    );
    const row = screen.getByText("docs").closest(".directory-picker-row")!;
    await fireEvent.click(within(row as HTMLElement).getByRole("button", { name: "选择" }));
    await fireEvent.click(screen.getByRole("button", { name: "保存" }));

    await waitFor(() =>
      expect(
        screen.getByText("密码长度需在 6-256 个字符之间"),
      ).toBeInTheDocument(),
    );
  });

  it("deletes only after confirmation", async () => {
    renderPage();
    await waitFor(() =>
      expect(screen.getByText("read-docs")).toBeInTheDocument(),
    );

    await fireEvent.click(screen.getByRole("button", { name: "删除" }));

    await waitFor(() =>
      expect(deleteMock).toHaveBeenCalledWith("c-1"),
    );
    expect(confirmMock).toHaveBeenCalledWith(
      expect.objectContaining({ title: "删除访问密码", danger: true }),
    );
  });
});
