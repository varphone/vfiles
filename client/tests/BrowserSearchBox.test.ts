import { fireEvent, render, screen } from "@testing-library/vue";
import { describe, expect, it, vi } from "vitest";
import BrowserSearchBox from "../src/components/file-browser/BrowserSearchBox.vue";

function renderBox(props: Record<string, unknown> = {}) {
  return render(BrowserSearchBox as any, {
    props: {
      modelValue: "",
      open: false,
      content: false,
      type: "all",
      scopeCurrent: false,
      loading: false,
      active: false,
      contentEnabled: true,
      filtersActive: false,
      history: ["报告", "notes"],
      ...props,
    },
  });
}

describe("BrowserSearchBox.vue", () => {
  it("describes what will be searched", async () => {
    const { unmount } = renderBox();
    expect(
      screen.getByPlaceholderText("搜索名称、扩展名或路径"),
    ).toBeInTheDocument();
    unmount();

    renderBox({ content: true });
    expect(
      screen.getByPlaceholderText("搜索当前工作区中的文本内容"),
    ).toBeInTheDocument();
  });

  it("emits input changes and the search action", async () => {
    const { emitted } = renderBox();
    const input = screen.getByPlaceholderText("搜索名称、扩展名或路径");

    await fireEvent.update(input, "报告");
    expect(emitted()["update:modelValue"]?.[0]).toEqual(["报告"]);

    await fireEvent.keyUp(input, { key: "Enter" });
    expect(emitted()["search"]).toHaveLength(1);

    await fireEvent.click(screen.getByText("搜索"));
    expect(emitted()["search"]).toHaveLength(2);
  });

  it("toggles the advanced panel", async () => {
    const { emitted } = renderBox({ open: false });
    await fireEvent.click(screen.getByLabelText("高级搜索"));
    expect(emitted()["update:open"]?.[0]).toEqual([true]);
  });

  it("only renders the filters when open", () => {
    const { unmount } = renderBox({ open: false });
    expect(screen.queryByText("全文搜索")).toBeNull();
    unmount();

    renderBox({ open: true });
    expect(screen.getByText("全文搜索")).toBeInTheDocument();
    expect(screen.getByText("仅当前目录")).toBeInTheDocument();
  });

  it("emits filter changes", async () => {
    const { emitted } = renderBox({ open: true });

    await fireEvent.click(screen.getByLabelText("全文搜索"));
    expect(emitted()["update:content"]?.[0]).toEqual([true]);

    await fireEvent.update(
      screen.getByRole("combobox", { name: "搜索类型" }),
      "directory",
    );
    expect(emitted()["update:type"]?.[0]).toEqual(["directory"]);

    await fireEvent.click(screen.getByLabelText("仅当前目录"));
    expect(emitted()["update:scopeCurrent"]?.[0]).toEqual([true]);

    await fireEvent.click(screen.getByText("清空搜索"));
    expect(emitted()["clear"]).toHaveLength(1);
  });

  it("disables full-text search when the feature is off", () => {
    renderBox({ open: true, contentEnabled: false });

    expect(screen.getByText("(未启用)")).toBeInTheDocument();
    expect(
      (screen.getByLabelText("全文搜索") as HTMLInputElement).disabled,
    ).toBe(true);
  });

  it("registers the input element for focusing", () => {
    const registerInput = vi.fn();
    const { unmount } = renderBox({ registerInput });

    expect(registerInput).toHaveBeenCalled();
    const lastRegistered = () =>
      registerInput.mock.calls[registerInput.mock.calls.length - 1]?.[0];
    expect(lastRegistered()).toBeInstanceOf(HTMLInputElement);

    unmount();
    expect(lastRegistered()).toBeNull();
  });

  it("closes the panel when clicking outside", async () => {
    const { emitted } = renderBox({ open: true });

    await fireEvent.click(document.body);
    expect(emitted()["update:open"]?.[0]).toEqual([false]);
  });

  it("labels the input for screen readers (r68)", () => {
    // placeholder 不是 label（屏读不播报）→ aria-label 与占位符同步
    renderBox();
    const input = screen.getByPlaceholderText(
      "搜索名称、扩展名或路径",
    ) as HTMLInputElement;
    expect(input.getAttribute("aria-label")).toBe("搜索名称、扩展名或路径");
  });

  it("emits clear on Escape and enter-results on ArrowDown (r62)", async () => {
    const { emitted } = renderBox({ modelValue: "搜索" });
    const input = screen.getByPlaceholderText("搜索名称、扩展名或路径");

    // 搜索框动线（主流：Esc 清搜索 / ↓ 进入结果首项）
    await fireEvent.keyDown(input, { key: "Escape" });
    expect(emitted()["clear"]).toHaveLength(1);

    await fireEvent.keyDown(input, { key: "ArrowDown" });
    expect(emitted()["enter-results"]).toHaveLength(1);
  });

  it("closes only the panel on Escape while it is open (r63 layering)", async () => {
    // 层级纪律（r30 同款）：面板开着时 Esc 只关层面板，输入/搜索保留；
    // 面板关着再按 Esc 才清空搜索。
    const { emitted } = renderBox({ modelValue: "搜索", open: true });
    const input = screen.getByPlaceholderText("搜索名称、扩展名或路径");

    await fireEvent.keyDown(input, { key: "Escape" });
    expect(emitted()["update:open"]?.[0]).toEqual([false]);
    expect(emitted()["clear"]).toBeUndefined();
  });
});
