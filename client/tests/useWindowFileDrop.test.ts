import { describe, expect, it, vi } from "vitest";
import { defineComponent, h, ref } from "vue";
import { render } from "@testing-library/vue";
import { useWindowFileDrop } from "../src/composables/useWindowFileDrop";

/** 把组合式函数挂到一个最小组件上，便于触发 window 事件。 */
function mountDrop(options: Parameters<typeof useWindowFileDrop>[0]) {
  let dragging!: ReturnType<typeof useWindowFileDrop>["dragging"];
  const Host = defineComponent({
    setup() {
      const state = useWindowFileDrop(options);
      dragging = state.dragging;
      return () => h("div", { "data-testid": "host" });
    },
  });
  render(Host);
  return { dragging: () => dragging.value };
}

/** jsdom 不实现 DataTransfer，直接挂在事件对象上即可。 */
function dispatch(type: string, types: string[], files: File[] = []) {
  const event = new Event(type, { cancelable: true, bubbles: true });
  Object.assign(event, {
    dataTransfer: { types, files, dropEffect: "" },
  });
  window.dispatchEvent(event);
  return event;
}

function sampleFile(name: string) {
  return new File(["data"], name, { type: "text/plain" });
}

describe("useWindowFileDrop", () => {
  it("shows the overlay while external files are dragged in", () => {
    const { dragging } = mountDrop({ onFiles: vi.fn() });

    expect(dragging()).toBe(false);
    dispatch("dragenter", ["Files"]);
    expect(dragging()).toBe(true);

    // 子元素间的 dragenter/dragleave 不应让浮层闪烁
    dispatch("dragenter", ["Files"]);
    dispatch("dragleave", ["Files"]);
    expect(dragging()).toBe(true);

    dispatch("dragleave", ["Files"]);
    expect(dragging()).toBe(false);
  });

  it("ignores internal drags that carry no files", () => {
    const onFiles = vi.fn();
    const { dragging } = mountDrop({ onFiles });

    dispatch("dragenter", ["text/plain"]);
    expect(dragging()).toBe(false);

    dispatch("drop", ["text/plain"]);
    expect(onFiles).not.toHaveBeenCalled();
  });

  it("hands the dropped files over and hides the overlay", () => {
    const onFiles = vi.fn();
    const { dragging } = mountDrop({ onFiles });

    dispatch("dragenter", ["Files"]);
    dispatch("drop", ["Files"], [sampleFile("a.txt"), sampleFile("b.txt")]);

    expect(onFiles).toHaveBeenCalledTimes(1);
    expect(onFiles.mock.calls[0][0].map((file: File) => file.name)).toEqual([
      "a.txt",
      "b.txt",
    ]);
    expect(dragging()).toBe(false);
  });

  it("prevents the browser from opening dropped files", () => {
    mountDrop({ onFiles: vi.fn() });

    expect(
      dispatch("drop", ["Files"], [sampleFile("a.txt")]).defaultPrevented,
    ).toBe(true);
  });

  it("keeps dragover prevented so the drop event fires", () => {
    mountDrop({ onFiles: vi.fn() });

    expect(dispatch("dragover", ["Files"]).defaultPrevented).toBe(true);
    expect(dispatch("dragover", ["text/plain"]).defaultPrevented).toBe(false);
  });

  it("stays disabled while a dialog owns the drop", () => {
    const onFiles = vi.fn();
    const enabled = ref(false);
    const { dragging } = mountDrop({ onFiles, enabled: () => enabled.value });

    dispatch("dragenter", ["Files"]);
    expect(dragging()).toBe(false);

    dispatch("drop", ["Files"], [sampleFile("a.txt")]);
    expect(onFiles).not.toHaveBeenCalled();

    enabled.value = true;
    dispatch("dragenter", ["Files"]);
    expect(dragging()).toBe(true);
  });
});
