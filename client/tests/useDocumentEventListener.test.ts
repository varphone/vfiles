import { describe, expect, it, vi } from "vitest";
import { defineComponent, h } from "vue";
import { render } from "@testing-library/vue";
import { useDocumentEventListener } from "../src/composables/useDocumentEventListener";

describe("useDocumentEventListener", () => {
  it("removes the document listener before its component is unmounted", () => {
    const listener = vi.fn();
    const Host = defineComponent({
      setup() {
        useDocumentEventListener("scroll", listener, true);
        return () => h("div");
      },
    });
    const { unmount } = render(Host);

    document.dispatchEvent(new Event("scroll"));
    expect(listener).toHaveBeenCalledTimes(1);

    unmount();
    document.dispatchEvent(new Event("scroll"));
    expect(listener).toHaveBeenCalledTimes(1);
  });
});
