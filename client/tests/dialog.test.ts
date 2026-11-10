import { describe, expect, it } from "vitest";
import {
  confirmDialog,
  dialogState,
  resolveDialog,
} from "../src/composables/dialog";


describe("对话框队列", () => {
  it("连续两次确认会依次出队，前一个 Promise 不会悬挂", async () => {
    const first = confirmDialog({ message: "第一次" });
    const second = confirmDialog({ message: "第二次" });

    expect(dialogState.value?.message).toBe("第一次");

    resolveDialog(false);
    await expect(first).resolves.toBe(false);
    expect(dialogState.value?.message).toBe("第二次");

    resolveDialog(true);
    await expect(second).resolves.toBe(true);
    expect(dialogState.value).toBeNull();
  });
});
