import { fireEvent } from "@testing-library/vue";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { renderWithProviders } from "./renderWithProviders";
import ThemeToggle from "../src/components/common/ThemeToggle.vue";
import { useThemeStore } from "../src/stores/theme.store";

const STORAGE_KEY = "vfiles:theme";

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

beforeEach(() => {
  installLocalStorageStub();
  delete document.documentElement.dataset.theme;
  vi.stubGlobal(
    "matchMedia",
    vi.fn(() => ({
      matches: false,
      media: "(prefers-color-scheme: dark)",
      onchange: null,
      addEventListener: () => {},
      removeEventListener: () => {},
      addListener: () => {},
      removeListener: () => {},
      dispatchEvent: () => false,
    })),
  );
});

describe("ThemeToggle.vue", () => {
  it("shows the current mode and switches the theme", async () => {
    const { findByRole, getByRole } = renderWithProviders(ThemeToggle as any);

    await fireEvent.click(getByRole("button", { name: "主题：跟随系统" }));
    await fireEvent.click(getByRole("menuitemradio", { name: "深色" }));

    expect(document.documentElement.dataset.theme).toBe("dark");
    expect(localStorage.getItem(STORAGE_KEY)).toBe("dark");
    // 菜单选择后收起，按钮标题更新为当前模式
    await findByRole("button", { name: "主题：深色" });
  });

  it("marks the active option as checked", async () => {
    localStorage.setItem(STORAGE_KEY, "light");
    const { getByRole } = renderWithProviders(ThemeToggle as any);

    await fireEvent.click(getByRole("button", { name: "主题：浅色" }));

    expect(getByRole("menuitemradio", { name: "跟随系统" })).toHaveAttribute(
      "aria-checked",
      "false",
    );
    expect(getByRole("menuitemradio", { name: "浅色" })).toHaveAttribute(
      "aria-checked",
      "true",
    );
  });

  it("sets the store mode through the shared store", async () => {
    const { getByRole } = renderWithProviders(ThemeToggle as any);
    // 组件用的是 renderWithProviders 内部创建的 pinia，需在 render 之后取同一个 store
    const store = useThemeStore();

    await fireEvent.click(getByRole("button", { name: "主题：跟随系统" }));
    await fireEvent.click(getByRole("menuitemradio", { name: "跟随系统" }));

    expect(store.mode).toBe("system");
  });
});
