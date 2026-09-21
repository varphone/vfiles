import { render, waitFor } from "@testing-library/vue";
import { describe, expect, it, vi } from "vitest";
import DropZone from "../src/components/file-uploader/DropZone.vue";

describe("DropZone.vue initial picker", () => {
  it("does not open a picker by default", async () => {
    const { container } = render(DropZone as any);
    const fileInput = container.querySelector<HTMLInputElement>(
      'input[type="file"]:not([webkitdirectory])',
    )!;
    const dirInput = container.querySelector<HTMLInputElement>(
      "input[webkitdirectory]",
    )!;
    const fileSpy = vi.spyOn(fileInput, "click");
    const dirSpy = vi.spyOn(dirInput, "click");

    await waitFor(() =>
      expect(container.querySelector(".drop-zone")).not.toBeNull(),
    );
    expect(fileSpy).not.toHaveBeenCalled();
    expect(dirSpy).not.toHaveBeenCalled();
  });

  it("opens the directory picker when asked from the upload menu", async () => {
    const { container } = render(DropZone as any, {
      props: { initialPick: "directory" },
    });
    const dirInput = container.querySelector<HTMLInputElement>(
      "input[webkitdirectory]",
    )!;
    const dirSpy = vi.spyOn(dirInput, "click");

    // 挂载后（nextTick）自动触发目录选择器
    await waitFor(() => expect(dirInput).not.toBeNull());
    await Promise.resolve();
    await waitFor(() => expect(dirSpy).toHaveBeenCalledTimes(1));
  });

  it("opens the picker when the dialog is opened later (content stays mounted)", async () => {
    const { container, rerender } = render(DropZone as any, {
      props: { initialPick: null },
    });
    const dirInput = container.querySelector<HTMLInputElement>(
      "input[webkitdirectory]",
    )!;
    const dirSpy = vi.spyOn(dirInput, "click");
    expect(dirSpy).not.toHaveBeenCalled();

    // 模拟 Modal 内容常驻：属性从 null 变为 directory
    await rerender({ initialPick: "directory" });
    await waitFor(() => expect(dirSpy).toHaveBeenCalledTimes(1));
  });

  it("opens the file picker for the primary upload action", async () => {
    const { container } = render(DropZone as any, {
      props: { initialPick: "files" },
    });
    const fileInput = container.querySelector<HTMLInputElement>(
      'input[type="file"]:not([webkitdirectory])',
    )!;
    const fileSpy = vi.spyOn(fileInput, "click");

    await Promise.resolve();
    await waitFor(() => expect(fileSpy).toHaveBeenCalledTimes(1));
  });
});
