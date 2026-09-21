import { fireEvent, render, screen, waitFor } from "@testing-library/vue";
import { beforeEach, describe, expect, it, vi } from "vitest";
import TransferOwnershipDialog from "../src/components/file-browser/TransferOwnershipDialog.vue";

const { listTargetsMock, confirmMock } = vi.hoisted(() => ({
  listTargetsMock: vi.fn(),
  confirmMock: vi.fn(),
}));

vi.mock("../src/services/files.service", () => ({
  filesService: {
    listTransferTargets: listTargetsMock,
  },
}));

vi.mock("../src/composables/dialog", () => ({
  confirmDialog: confirmMock,
}));

const items = [
  { name: "交接.txt", path: "docs/交接.txt" },
  { name: "预算.csv", path: "docs/预算.csv" },
];

function renderDialog(props: Record<string, unknown> = {}) {
  return render(TransferOwnershipDialog as any, {
    props: {
      show: true,
      items,
      ...props,
    },
  });
}

describe("TransferOwnershipDialog.vue", () => {
  beforeEach(() => {
    listTargetsMock.mockReset();
    listTargetsMock.mockResolvedValue([
      { id: "u-1", username: "alice" },
      { id: "u-2", username: "bob" },
    ]);
    confirmMock.mockReset();
    confirmMock.mockResolvedValue(true);
  });

  it("lists transfer targets and shows what will be transferred", async () => {
    renderDialog();

    await waitFor(() => expect(screen.getByText("alice")).toBeInTheDocument());
    expect(screen.getByText("bob")).toBeInTheDocument();
    expect(screen.getByText("交接.txt、预算.csv")).toBeInTheDocument();
    // 明确提示版本历史随行
    expect(screen.getByText(/版本历史会一起转移/)).toBeInTheDocument();
  });

  it("filters targets by the search box", async () => {
    renderDialog();
    await waitFor(() => expect(screen.getByText("alice")).toBeInTheDocument());

    await fireEvent.update(screen.getByLabelText("搜索接收用户"), "bo");
    await waitFor(() =>
      expect(screen.queryByText("alice")).not.toBeInTheDocument(),
    );
    expect(screen.getByText("bob")).toBeInTheDocument();
  });

  it("emits transfer with the selected target and message after confirmation", async () => {
    const { emitted } = renderDialog();
    await waitFor(() => expect(screen.getByText("alice")).toBeInTheDocument());

    // 未选择用户时按钮不可用（标题也是「转移所有权」，这里按文本精确取按钮）
    const submit = screen
      .getAllByText("转移所有权")
      .map((node) => node.closest("button"))
      .find((button) => button !== null)!;
    expect(submit).toBeDisabled();

    await fireEvent.click(screen.getByText("alice"));
    await fireEvent.update(
      document.querySelector("#transfer-message")!,
      "项目交接",
    );
    await fireEvent.click(submit);

    await waitFor(() => expect(confirmMock).toHaveBeenCalled());
    const transfers = emitted().transfer as unknown[][];
    expect(transfers).toHaveLength(1);
    expect(transfers[0][0]).toEqual({ id: "u-1", username: "alice" });
    expect(transfers[0][1]).toBe("项目交接");
  });

  it("does not emit when the confirmation is declined", async () => {
    confirmMock.mockResolvedValue(false);
    const { emitted } = renderDialog();
    await waitFor(() => expect(screen.getByText("alice")).toBeInTheDocument());

    await fireEvent.click(screen.getByText("alice"));
    await fireEvent.click(
      screen
        .getAllByText("转移所有权")
        .map((node) => node.closest("button"))
        .find((button) => button !== null)!,
    );

    await waitFor(() => expect(confirmMock).toHaveBeenCalled());
    expect(emitted().transfer).toBeUndefined();
  });

  it("shows an empty state when there is nobody else to transfer to", async () => {
    listTargetsMock.mockResolvedValue([]);
    renderDialog();

    await waitFor(() =>
      expect(screen.getByText("没有其他可接收的用户")).toBeInTheDocument(),
    );
  });

  it("surfaces a load failure", async () => {
    listTargetsMock.mockRejectedValue(new Error("网络错误"));
    renderDialog();

    await waitFor(() => expect(screen.getByRole("alert")).toBeInTheDocument());
    expect(screen.getByRole("alert").textContent).toContain("网络错误");
  });
});
