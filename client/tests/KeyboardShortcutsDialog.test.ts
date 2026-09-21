import { render, screen } from "@testing-library/vue";
import { describe, expect, it } from "vitest";
import KeyboardShortcutsDialog from "../src/components/file-browser/KeyboardShortcutsDialog.vue";

function renderDialog(show = true) {
  return render(KeyboardShortcutsDialog as any, { props: { show } });
}

describe("KeyboardShortcutsDialog.vue", () => {
  it("groups shortcuts and renders keys as kbd elements", () => {
    const { container } = renderDialog();

    const titles = Array.from(
      container.querySelectorAll(".shortcuts-group-title"),
    ).map((el) => el.textContent?.trim());
    expect(titles).toEqual(["导航", "选择", "操作", "预览与搜索"]);

    const keys = Array.from(container.querySelectorAll(".shortcuts-keys")).map(
      (el) =>
        Array.from(el.querySelectorAll("kbd")).map((k) =>
          k.textContent?.trim(),
        ),
    );
    expect(keys).toContainEqual(["Ctrl", "A"]);
    expect(keys).toContainEqual(["Shift", "F10"]);
    expect(keys).toContainEqual(["F2"]);
  });

  it("documents the most-used shortcuts", () => {
    renderDialog();

    expect(screen.getByText("重命名当前条目")).toBeInTheDocument();
    expect(
      screen.getByText("删除选中条目（Backspace 同效）"),
    ).toBeInTheDocument();
    expect(screen.getByText("打开文件夹，或预览文件")).toBeInTheDocument();
    expect(screen.getByText(/打开 \/ 关闭本快捷键面板/)).toBeInTheDocument();
  });

  it("mentions the ? shortcut in the intro", () => {
    const { container } = renderDialog();

    expect(container.querySelector(".shortcuts-hint")?.textContent).toContain(
      "?",
    );
  });
});
