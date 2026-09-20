import { fireEvent, render, screen } from "@testing-library/vue";
import { beforeEach, describe, expect, it } from "vitest";
import { createPinia, setActivePinia } from "pinia";
import SortMenu from "../src/components/file-browser/SortMenu.vue";
import { useFileViewStore } from "../src/stores/fileView.store";

function installLocalStorageStub() {
  const backing = new Map<string, string>();
  Object.defineProperty(globalThis, "localStorage", {
    value: {
      getItem: (key: string) => (backing.has(key) ? backing.get(key)! : null),
      setItem: (key: string, value: string) => backing.set(key, String(value)),
      removeItem: (key: string) => backing.delete(key),
      clear: () => backing.clear(),
      key: (index: number) => Array.from(backing.keys())[index] ?? null,
      get length() {
        return backing.size;
      },
    },
    configurable: true,
    writable: true,
  });
}

function renderMenu() {
  const pinia = createPinia();
  setActivePinia(pinia);
  render(SortMenu as any, { global: { plugins: [pinia] } });
  return useFileViewStore();
}

describe("SortMenu.vue", () => {
  beforeEach(() => {
    installLocalStorageStub();
  });

  it("reflects the current field and direction", () => {
    renderMenu();

    // 触发器在桌面显示字段名，title 带完整的「字段 + 方向」描述
    expect(screen.getByTitle("排序：名称 升序")).toBeInTheDocument();
    expect(screen.getByLabelText("升序")).toBeInTheDocument();
  });

  it("switches the sort field from the menu", async () => {
    const view = renderMenu();

    await fireEvent.click(screen.getByTitle("排序：名称 升序"));
    await fireEvent.click(
      screen.getByRole("menuitemradio", { name: "修改时间" }),
    );

    expect(view.sortField).toBe("modified");
    // 选择后菜单收起（Bulma 下拉保留 DOM，用 is-active 判断开合）
    expect(
      document.querySelector(".sort-menu")?.classList.contains("is-active"),
    ).toBe(false);
  });

  it("toggles the direction when the same field is chosen again", async () => {
    const view = renderMenu();

    await fireEvent.click(screen.getByTitle("排序：名称 升序"));
    await fireEvent.click(screen.getByRole("menuitemradio", { name: "名称" }));

    expect(view.sortField).toBe("name");
    expect(view.sortDirection).toBe("desc");
  });

  it("flips the direction from the arrow button", async () => {
    const view = renderMenu();

    await fireEvent.click(screen.getByLabelText("升序"));

    expect(view.sortDirection).toBe("desc");
    expect(screen.getByLabelText("降序")).toBeInTheDocument();
  });

  it("toggles folders-first from the menu", async () => {
    const view = renderMenu();
    expect(view.foldersFirst).toBe(true);

    await fireEvent.click(screen.getByTitle("排序：名称 升序"));
    await fireEvent.click(
      screen.getByRole("menuitemcheckbox", { name: "文件夹置顶" }),
    );

    expect(view.foldersFirst).toBe(false);
  });
});
