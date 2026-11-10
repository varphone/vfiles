import { fireEvent, render, screen, waitFor } from "@testing-library/vue";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { createPinia, setActivePinia } from "pinia";
import ProfileDialog from "../src/components/auth/ProfileDialog.vue";

const { updateProfileMock } = vi.hoisted(() => ({
  updateProfileMock: vi.fn(),
}));

vi.mock("../src/services/auth.service", () => ({
  authService: {
    updateProfile: updateProfileMock,
  },
}));

function renderDialog() {
  const pinia = createPinia();
  setActivePinia(pinia);
  const onClose = vi.fn();
  return {
    ...render(ProfileDialog as any, {
      props: { show: true, onClose },
      global: { plugins: [pinia] },
    }),
    onClose,
  };
}

beforeEach(() => {
  updateProfileMock.mockReset();
  updateProfileMock.mockResolvedValue({ success: true });
});

describe("ProfileDialog.vue", () => {
  it("rejects an empty email without calling the API", async () => {
    renderDialog();
    await fireEvent.update(screen.getByLabelText("邮箱"), "  ");
    await fireEvent.click(screen.getByRole("button", { name: "保存" }));

    expect(await screen.findByRole("alert")).toHaveTextContent("请输入邮箱");
    expect(updateProfileMock).not.toHaveBeenCalled();
  });

  it("saves the email and closes", async () => {
    const { onClose } = renderDialog();
    await fireEvent.update(screen.getByLabelText("邮箱"), "new@example.com");
    await fireEvent.click(screen.getByRole("button", { name: "保存" }));

    await waitFor(() =>
      expect(updateProfileMock).toHaveBeenCalledWith("new@example.com"),
    );
    expect(onClose).toHaveBeenCalledTimes(1);
  });
});
