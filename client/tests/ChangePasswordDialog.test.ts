import { fireEvent, render, screen, waitFor } from "@testing-library/vue";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { createPinia, setActivePinia } from "pinia";
import { createMemoryHistory, createRouter } from "vue-router";
import ChangePasswordDialog from "../src/components/auth/ChangePasswordDialog.vue";

const { changePasswordMock, logoutMock } = vi.hoisted(() => ({
  changePasswordMock: vi.fn(),
  logoutMock: vi.fn(),
}));

vi.mock("../src/services/auth.service", () => ({
  authService: {
    changePassword: changePasswordMock,
    logout: logoutMock,
  },
}));

const Blank = { render: () => null };

function renderDialog() {
  const pinia = createPinia();
  setActivePinia(pinia);
  const router = createRouter({
    history: createMemoryHistory(),
    routes: [
      { path: "/", name: "home", component: Blank },
      { path: "/login", name: "login", component: Blank },
    ],
  });
  const onClose = vi.fn();
  return {
    ...render(ChangePasswordDialog as any, {
      props: { show: true, onClose },
      global: { plugins: [pinia, router] },
    }),
    router,
    onClose,
  };
}

beforeEach(() => {
  changePasswordMock.mockReset();
  logoutMock.mockReset();
  logoutMock.mockResolvedValue({ success: true });
});

describe("ChangePasswordDialog.vue", () => {
  it("validates the confirmation before calling the API", async () => {
    renderDialog();

    await fireEvent.update(screen.getByLabelText("当前密码"), "old-password");
    await fireEvent.update(screen.getByLabelText("新密码"), "new-password");
    await fireEvent.update(screen.getByLabelText("确认新密码"), "different");
    await fireEvent.click(screen.getByRole("button", { name: "修改密码" }));

    expect(await screen.findByRole("alert")).toHaveTextContent(
      "两次输入的新密码不一致",
    );
    expect(changePasswordMock).not.toHaveBeenCalled();
  });

  it("changes the password, closes and returns to login", async () => {
    changePasswordMock.mockResolvedValue({ success: true });
    const { router, onClose } = renderDialog();

    await fireEvent.update(screen.getByLabelText("当前密码"), "old-password");
    await fireEvent.update(screen.getByLabelText("新密码"), "new-password");
    await fireEvent.update(screen.getByLabelText("确认新密码"), "new-password");
    await fireEvent.click(screen.getByRole("button", { name: "修改密码" }));

    await waitFor(() =>
      expect(changePasswordMock).toHaveBeenCalledWith(
        "old-password",
        "new-password",
      ),
    );
    expect(onClose).toHaveBeenCalledTimes(1);
    await waitFor(() => expect(logoutMock).toHaveBeenCalled());
    await waitFor(() =>
      expect(router.currentRoute.value.path).toBe("/login"),
    );
  });
});
