import { describe, it, expect } from "vitest";
import Login from "../src/views/Login.vue";
import { fireEvent, waitFor } from "@testing-library/vue";
import { renderWithProviders } from "./renderWithProviders";

describe("Login.vue", () => {
  it("renders login form by default", async () => {
    const { getByPlaceholderText, getByText } = renderWithProviders(Login);

    expect(getByText("VFiles")).toBeTruthy();
    expect(getByText("基于版本控制的文件管理系统")).toBeTruthy();
    // username input exists
    expect(getByPlaceholderText("3-32 位，字母数字-_")).toBeTruthy();
    // password input exists
    expect(getByPlaceholderText("至少 6 位")).toBeTruthy();
  });
});

describe("Login.vue auth shell", () => {
  it("switches modes with the segmented control", async () => {
    const { findByText, getByRole, queryByPlaceholderText } =
      renderWithProviders(Login);
    await findByText("VFiles");

    // 默认登录：显示用户名/密码，不显示邮箱验证码字段
    expect(queryByPlaceholderText("6 位验证码")).toBeNull();

    await fireEvent.click(getByRole("tab", { name: "邮箱验证码" }));
    await waitFor(() =>
      expect(queryByPlaceholderText("6 位验证码")).not.toBeNull(),
    );
    expect(queryByPlaceholderText("3-32 位，字母数字-_")).toBeNull();

    await fireEvent.click(getByRole("tab", { name: "登录" }));
    await waitFor(() =>
      expect(queryByPlaceholderText("3-32 位，字母数字-_")).not.toBeNull(),
    );
  });
});
