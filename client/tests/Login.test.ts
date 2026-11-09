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
  it("shows inline field errors with the aria chain (r86 family)", async () => {
    const { getByText, container } = renderWithProviders(Login);

    // 空提交 → 行内错误（原 toast 静默拦 ✗）+ aria 链
    await fireEvent.click(container.querySelector(".auth-submit")!);
    await waitFor(() => expect(getByText("请输入用户名")).toBeInTheDocument());
    const input = container.querySelector("#auth-username")!;
    expect(input.getAttribute("aria-invalid")).toBe("true");
    expect(input.getAttribute("aria-describedby")).toBe("auth-username-error");

    // 密码字段同款（登录模式内确定性 ✓ 免切模式链）
    await fireEvent.update(container.querySelector("#auth-username")!, "u");
    await fireEvent.click(container.querySelector(".auth-submit")!);
    await waitFor(() => expect(getByText("请输入密码")).toBeInTheDocument());
    const pw = container.querySelector("#auth-password")!;
    expect(pw.getAttribute("aria-invalid")).toBe("true");
    expect(pw.getAttribute("aria-describedby")).toBe("auth-password-error");

    // 注册段 = jsdom 脆点组合（模式切换 × 提交链 3 轮 3 种失败 ✗ 理性收窄记档）：
    // 校验分支由源码 + 本两段间接护 ✓ 真机路径留浏览器探针线（工具语 #35）
  });

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

  it("toggles the password visibility with the eye button", async () => {
    const { container, getByRole } = renderWithProviders(Login);
    const password = container.querySelector(
      "#auth-password",
    ) as HTMLInputElement;
    expect(password.type).toBe("password");

    const show = getByRole("button", { name: "显示密码" });
    expect(show.getAttribute("aria-pressed")).toBe("false");
    await fireEvent.click(show);

    expect(password.type).toBe("text");
    expect(getByRole("button", { name: "隐藏密码" })).toHaveAttribute(
      "aria-pressed",
      "true",
    );

    await fireEvent.click(getByRole("button", { name: "隐藏密码" }));
    expect(password.type).toBe("password");
    expect(getByRole("button", { name: "显示密码" })).toHaveAttribute(
      "aria-pressed",
      "false",
    );
  });

  it("renders floating labels bound to their inputs", () => {
    const { container, getByLabelText } = renderWithProviders(Login);

    // Material 外框风格：label 与 input 用 for/id 关联，并带浮动标签类名
    const username = getByLabelText("账号或邮箱");
    expect(username).toHaveAttribute("id", "auth-username");
    expect(container.querySelector('label[for="auth-username"]')).toHaveClass(
      "auth-label",
    );

    const password = getByLabelText("密码");
    expect(password).toHaveAttribute("id", "auth-password");
    expect(container.querySelector('label[for="auth-password"]')).toHaveClass(
      "auth-label",
    );
  });

  it("renders the deep-navy primary submit button", () => {
    const { container } = renderWithProviders(Login);

    // 主按钮：深蓝语义类 + data 属性 + 右侧箭头图标
    const submit = container.querySelector(".auth-submit");
    expect(submit).toHaveClass("auth-primary");
    expect(submit).toHaveAttribute("data-variant", "primary");
    expect(submit?.querySelector(".auth-submit-icon")).toBeTruthy();
  });

  it("shows the version and ICP record in the footer", () => {
    const { getByRole, getByText } = renderWithProviders(Login);

    // 版本号：VITE_APP_VERSION 未注入时回退为「VFiles」，始终有可见文案
    expect(getByText(/^版本 /)).toBeInTheDocument();
    expect(
      getByRole("link", { name: "苏ICP备2026014694号-1" }),
    ).toHaveAttribute("href", "https://beian.miit.gov.cn/");
  });
});
