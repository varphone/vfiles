import { render, screen } from "@testing-library/vue";
import { afterEach, describe, expect, it, vi } from "vitest";
import ContextMenu, {
  type ContextMenuItem,
} from "../src/components/file-browser/ContextMenu.vue";

const originalInnerHeight = window.innerHeight;

afterEach(() => {
  vi.restoreAllMocks();
  Object.defineProperty(window, "innerHeight", {
    configurable: true,
    value: originalInnerHeight,
  });
});

describe("ContextMenu.vue placement", () => {
  it("opens upward when the file row is near the bottom of the viewport", async () => {
    Object.defineProperty(window, "innerHeight", {
      configurable: true,
      value: 600,
    });
    vi.spyOn(HTMLElement.prototype, "getBoundingClientRect").mockReturnValue({
      x: 100,
      y: 0,
      width: 184,
      height: 200,
      top: 0,
      right: 284,
      bottom: 200,
      left: 100,
      toJSON: () => ({}),
    } as DOMRect);

    const items: ContextMenuItem[] = [
      { key: "download", label: "下载" },
      { key: "rename", label: "重命名" },
    ];
    render(ContextMenu, {
      props: { show: true, x: 100, y: 580, items },
    });

    await new Promise((resolve) => setTimeout(resolve, 0));

    const menu = screen.getByRole("menu");
    expect(menu.style.top).toBe("380px");
    expect(menu.style.maxHeight).toBe("calc(100dvh - 16px)");
    expect(menu.style.overflowY).toBe("auto");
  });
});
