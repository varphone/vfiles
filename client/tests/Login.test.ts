import { describe, it, expect } from "vitest";
import Login from "../src/views/Login.vue";
import { renderWithProviders } from "./renderWithProviders";

describe("Login.vue", () => {
  it("renders login form by default", async () => {
    const { getByPlaceholderText, getByText } = renderWithProviders(Login);

    expect(getByText("VFiles 登录")).toBeTruthy();
    // username input exists
    expect(getByPlaceholderText("3-32 位，字母数字-_")).toBeTruthy();
    // password input exists
    expect(getByPlaceholderText("至少 6 位")).toBeTruthy();
  });
});
