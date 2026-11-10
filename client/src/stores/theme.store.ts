import { defineStore } from "pinia";
import { computed, ref } from "vue";

export type ThemeMode = "system" | "light" | "dark";
export type ResolvedTheme = "light" | "dark";

const STORAGE_KEY = "vfiles:theme";
const DARK_MEDIA_QUERY = "(prefers-color-scheme: dark)";
const MODES: ThemeMode[] = ["system", "light", "dark"];

function readPersistedMode(): ThemeMode {
  if (typeof localStorage === "undefined") return "system";

  try {
    const raw = localStorage.getItem(STORAGE_KEY);
    return MODES.includes(raw as ThemeMode) ? (raw as ThemeMode) : "system";
  } catch {
    return "system";
  }
}

function systemPrefersDark(): boolean {
  if (typeof window === "undefined" || !window.matchMedia) return false;

  try {
    return window.matchMedia(DARK_MEDIA_QUERY).matches;
  } catch {
    return false;
  }
}

export const useThemeStore = defineStore("theme", () => {
  const mode = ref<ThemeMode>(readPersistedMode());
  const systemDark = ref(systemPrefersDark());
  const resolved = computed<ResolvedTheme>(() =>
    mode.value === "system"
      ? systemDark.value
        ? "dark"
        : "light"
      : mode.value,
  );

  /** 把当前主题写到 <html data-theme> 与浏览器地址栏颜色上。 */
  function apply() {
    if (typeof document === "undefined") return;

    const root = document.documentElement;
    // system 模式移除属性，交由 prefers-color-scheme 决定。
    if (mode.value === "system") {
      delete root.dataset.theme;
    } else {
      root.dataset.theme = mode.value;
    }

    // 地址栏颜色从令牌读取，避免在 JS 里维护第二份色值。
    const meta = document.querySelector('meta[name="theme-color"]');
    const shell = getComputedStyle(root)
      .getPropertyValue("--vf-shell-bg")
      .trim();
    if (meta && shell) meta.setAttribute("content", shell);
  }

  function setMode(next: ThemeMode) {
    if (!MODES.includes(next)) return;
    mode.value = next;

    try {
      if (next === "system") {
        localStorage.removeItem(STORAGE_KEY);
      } else {
        localStorage.setItem(STORAGE_KEY, next);
      }
    } catch {
      /* 隐私模式下写失败不影响本次会话 */
    }

    apply();
  }

  /** 在浅色/深色/跟随系统之间循环，供紧凑的切换按钮使用。 */
  function cycle() {
    const order: ThemeMode[] = ["system", "light", "dark"];
    const index = order.indexOf(mode.value);
    setMode(order[(index + 1) % order.length]);
  }

  let mediaQueryList: MediaQueryList | null = null;
  let mediaListener: ((event: MediaQueryListEvent) => void) | null = null;

  /** 监听系统配色变化；应用启动时调用一次即可。 */
  function init() {
    apply();
    if (typeof window === "undefined" || !window.matchMedia) return;
    if (mediaQueryList) return;

    try {
      mediaQueryList = window.matchMedia(DARK_MEDIA_QUERY);
      mediaListener = (event: MediaQueryListEvent) => {
        systemDark.value = event.matches;
        if (mode.value === "system") apply();
      };

      if (typeof mediaQueryList.addEventListener === "function") {
        mediaQueryList.addEventListener("change", mediaListener);
      } else if (typeof mediaQueryList.addListener === "function") {
        mediaQueryList.addListener(mediaListener);
      }
    } catch {
      mediaQueryList = null;
      mediaListener = null;
    }
  }

  return {
    mode,
    resolved,
    systemDark,
    setMode,
    cycle,
    apply,
    init,
  };
});
