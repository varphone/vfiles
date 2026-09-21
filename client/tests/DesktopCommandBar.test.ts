import { fireEvent, render, screen, waitFor } from "@testing-library/vue";
import { describe, expect, it, vi } from "vitest";
import { createPinia, setActivePinia } from "pinia";
import DesktopCommandBar from "../src/components/file-browser/DesktopCommandBar.vue";

vi.mock("../src/services/files.service", () => ({
  SEARCH_PAGE_SIZE: 100,
  filesService: {},
}));

function renderBar() {
  const pinia = createPinia();
  setActivePinia(pinia);
  return render(DesktopCommandBar as any, {
    global: { plugins: [pinia] },
    props: {
      searchQuery: "",
      searchOpen: false,
      searchContent: false,
      searchType: "name",
      searchScopeCurrent: false,
    },
  });
}

describe("DesktopCommandBar.vue upload split button", () => {
  it("keeps 上传 as the primary action and lists the alternatives behind the caret", async () => {
    const { container } = renderBar();
    const isOpen = () =>
      Boolean(container.querySelector(".desktop-upload-split.is-active"));

    // 主按钮就是「上传」，次级方式收在下拉里
    expect(screen.getByRole("button", { name: "上传" })).toBeInTheDocument();
    expect(isOpen()).toBe(false);

    await fireEvent.click(screen.getByLabelText("更多上传方式"));
    await waitFor(() => expect(isOpen()).toBe(true));
    expect(
      Array.from(
        container.querySelectorAll(".desktop-upload-menu [role='menuitem']"),
      ).map((item) => item.textContent?.trim()),
    ).toEqual(["上传文件", "上传文件夹", "新建文件夹"]);
  });

  it("emits upload-files / upload-folder / create-folder from the menu", async () => {
    const { container, emitted } = renderBar();
    const open = async () =>
      fireEvent.click(screen.getByLabelText("更多上传方式"));

    await open();
    await fireEvent.click(screen.getByText("上传文件夹"));
    expect(emitted()["upload-folder"]).toHaveLength(1);

    await open();
    await fireEvent.click(screen.getByText("上传文件"));
    expect(emitted()["upload-files"]).toHaveLength(1);

    await open();
    await fireEvent.click(
      container.querySelector(
        ".desktop-upload-menu [role='menuitem']:last-of-type",
      )!,
    );
    expect(emitted()["create-folder"]).toHaveLength(1);
  });

  it("closes the menu on outside click or Escape", async () => {
    const { container } = renderBar();
    const isOpen = () =>
      Boolean(container.querySelector(".desktop-upload-split.is-active"));

    await fireEvent.click(screen.getByLabelText("更多上传方式"));
    await waitFor(() => expect(isOpen()).toBe(true));
    await fireEvent.keyDown(document, { key: "Escape" });
    await waitFor(() => expect(isOpen()).toBe(false));

    await fireEvent.click(screen.getByLabelText("更多上传方式"));
    await waitFor(() => expect(isOpen()).toBe(true));
    await fireEvent.click(document.body);
    await waitFor(() => expect(isOpen()).toBe(false));
  });
});
