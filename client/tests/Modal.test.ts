import { render, fireEvent, waitFor } from "@testing-library/vue";
import { h, nextTick } from "vue";
import { describe, it, expect } from "vitest";
import Modal from "../src/components/common/Modal.vue";

describe("Modal.vue focus trap (r131)", () => {
  function renderModal() {
    return render(Modal, {
      props: { show: true, title: "测试对话框" },
      slots: {
        default: () => [
          h("button", { class: "first-btn" }, "首个"),
          h("button", { class: "last-btn" }, "末个"),
        ],
        footer: () => h("button", { class: "footer-btn" }, "确定"),
      },
    });
  }

  it("moves focus into the dialog on open (first focusable = head close)", async () => {
    renderModal();
    await waitFor(() => {
      const active = document.activeElement as HTMLElement | null;
      expect(active?.closest(".modal-card")).not.toBeNull();
    });
  });

  it("cycles Tab within the dialog (first wraps to last)", async () => {
    renderModal();
    await waitFor(() => {
      const active = document.activeElement as HTMLElement | null;
      expect(active?.closest(".modal-card")).not.toBeNull();
    });
    // 环绕验证：DOM 序首个可聚焦 = 头部关闭按钮（初始焦点刻意跳过它 ✓ r132），
    // 手动聚焦后 Shift+Tab 应环绕到末尾（footer 确定）
    const close = document.querySelector(".modal-card-head .delete") as HTMLElement;
    close.focus();
    await fireEvent.keyDown(close, { key: "Tab", shiftKey: true });
    await nextTick();
    const active = document.activeElement as HTMLElement | null;
    expect(active?.className).toContain("footer-btn");
  });
});

describe("Modal.vue 层叠与语义", () => {
  it("叠加时只有最上层响应 Esc", async () => {
    const bottom = render(Modal, {
      props: { show: true, title: "底层" },
      slots: { default: "底层内容" },
    });
    const top = render(Modal, {
      props: { show: true, title: "上层" },
      slots: { default: "上层内容" },
    });

    await fireEvent.keyDown(document, { key: "Escape" });
    expect(top.emitted().close).toBeTruthy();
    expect(bottom.emitted().close).toBeUndefined();

    top.unmount();
    await fireEvent.keyDown(document, { key: "Escape" });
    expect(bottom.emitted().close).toBeTruthy();
  });

  it("暴露 dialog 语义与标题关联", () => {
    const { container } = render(Modal, {
      props: { show: true, title: "删除确认" },
      slots: { default: "内容" },
    });
    const dialog = container.querySelector(".modal")!;
    expect(dialog.getAttribute("role")).toBe("dialog");
    expect(dialog.getAttribute("aria-modal")).toBe("true");
    const labelledBy = dialog.getAttribute("aria-labelledby");
    expect(labelledBy).toBeTruthy();
    expect(container.querySelector(`#${labelledBy}`)?.textContent).toBe("删除确认");
  });
});
