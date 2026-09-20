import { beforeEach, describe, expect, it, vi } from "vitest";
import { createPinia, setActivePinia } from "pinia";
import { useThemeStore } from "../src/stores/theme.store";

const STORAGE_KEY = "vfiles:theme";

/** 测试环境（bun + jsdom）的 localStorage 不完整，这里提供确定性实现。 */
function installLocalStorageStub() {
  const backing = new Map<string, string>();
  const stub = {
    getItem: (key: string) => (backing.has(key) ? backing.get(key)! : null),
    setItem: (key: string, value: string) => {
      backing.set(key, String(value));
    },
    removeItem: (key: string) => {
      backing.delete(key);
    },
    clear: () => {
      backing.clear();
    },
    key: (index: number) => Array.from(backing.keys())[index] ?? null,
    get length() {
      return backing.size;
    },
  };
  Object.defineProperty(globalThis, "localStorage", {
    value: stub,
    configurable: true,
    writable: true,
  });
}

/** 可控的 matchMedia 桩，便于模拟系统在运行中切换配色。 */
function installMatchMediaStub(initialDark: boolean) {
  const listeners = new Set<(event: { matches: boolean }) => void>();
  let matches = initialDark;

  const list = {
    get matches() {
      return matches;
    },
    media: "(prefers-color-scheme: dark)",
    onchange: null,
    addEventListener: (_: string, listener: (event: never) => void) => {
      listeners.add(listener as never);
    },
    removeEventListener: (_: string, listener: (event: never) => void) => {
      listeners.delete(listener as never);
    },
    addListener: (listener: (event: never) => void) => {
      listeners.add(listener as never);
    },
    removeListener: (listener: (event: never) => void) => {
      listeners.delete(listener as never);
    },
    dispatchEvent: () => true,
  };

  vi.stubGlobal(
    "matchMedia",
    vi.fn(() => list as unknown as MediaQueryList),
  );

  return {
    setSystemDark(next: boolean) {
      matches = next;
      for (const listener of listeners) listener({ matches: next });
    },
  };
}

describe("theme store", () => {
  beforeEach(() => {
    installLocalStorageStub();
    installMatchMediaStub(false);
    delete document.documentElement.dataset.theme;
    setActivePinia(createPinia());
  });

  it("follows the system preference by default", () => {
    installMatchMediaStub(true);
    const store = useThemeStore();

    expect(store.mode).toBe("system");
    expect(store.resolved).toBe("dark");
    expect(store.systemDark).toBe(true);
  });

  it("applies an explicit light theme even when the system is dark", () => {
    installMatchMediaStub(true);
    const store = useThemeStore();

    store.setMode("light");

    expect(store.resolved).toBe("light");
    expect(document.documentElement.dataset.theme).toBe("light");
    expect(localStorage.getItem(STORAGE_KEY)).toBe("light");
  });

  it("persists dark mode and clears the attribute for system mode", () => {
    const store = useThemeStore();

    store.setMode("dark");
    expect(document.documentElement.dataset.theme).toBe("dark");
    expect(localStorage.getItem(STORAGE_KEY)).toBe("dark");

    store.setMode("system");
    expect(document.documentElement.dataset.theme).toBeUndefined();
    expect(localStorage.getItem(STORAGE_KEY)).toBeNull();
  });

  it("reads the persisted mode on creation", () => {
    localStorage.setItem(STORAGE_KEY, "dark");

    const store = useThemeStore();

    expect(store.mode).toBe("dark");
    expect(store.resolved).toBe("dark");
  });

  it("ignores unknown persisted values", () => {
    localStorage.setItem(STORAGE_KEY, "neon");

    const store = useThemeStore();

    expect(store.mode).toBe("system");
  });

  it("reacts to system changes only while following the system", () => {
    const media = installMatchMediaStub(false);
    const store = useThemeStore();
    store.init();

    media.setSystemDark(true);
    expect(store.resolved).toBe("dark");

    store.setMode("light");
    media.setSystemDark(false);
    media.setSystemDark(true);
    expect(store.resolved).toBe("light");
  });

  it("cycles system → light → dark", () => {
    const store = useThemeStore();

    store.cycle();
    expect(store.mode).toBe("light");
    store.cycle();
    expect(store.mode).toBe("dark");
    store.cycle();
    expect(store.mode).toBe("system");
  });
});
