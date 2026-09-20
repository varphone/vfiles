import { fireEvent, render, screen } from "@testing-library/vue";
import { describe, expect, it } from "vitest";
import BatchActionBar from "../src/components/file-browser/BatchActionBar.vue";

function renderBar(selectedCount: number) {
  return render(BatchActionBar as any, { props: { selectedCount } });
}

function button(label: string) {
  return screen.getByText(label).closest("button") as HTMLButtonElement;
}

describe("BatchActionBar.vue", () => {
  it("shows how many entries are selected", () => {
    renderBar(3);
    expect(screen.getByText("已选 3 项")).toBeInTheDocument();
  });

  it("disables the mutating actions when nothing is selected", () => {
    renderBar(0);

    for (const label of ["下载", "移动", "删除", "重命名"]) {
      expect(button(label).disabled).toBe(true);
    }
    // 全选/清空始终可用
    expect(button("全选当前视图").disabled).toBe(false);
    expect(button("清空选择").disabled).toBe(false);
  });

  it("enables bulk actions for one or more entries", () => {
    renderBar(1);

    expect(button("下载").disabled).toBe(false);
    expect(button("移动").disabled).toBe(false);
    expect(button("删除").disabled).toBe(false);
    expect(button("重命名").disabled).toBe(false);
  });

  it("keeps rename limited to a single entry", () => {
    renderBar(2);

    expect(button("重命名").disabled).toBe(true);
    expect(button("下载").disabled).toBe(false);
  });

  it("emits the action for each button", async () => {
    const { emitted } = renderBar(2);

    const actions: [string, string][] = [
      ["全选当前视图", "select-all"],
      ["清空选择", "clear-selection"],
      ["下载", "download"],
      ["移动", "move"],
      ["删除", "delete"],
    ];

    for (const [label, event] of actions) {
      await fireEvent.click(screen.getByText(label));
      expect(emitted()[event]).toHaveLength(1);
    }
  });
});
