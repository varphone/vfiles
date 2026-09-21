import { fireEvent, render, screen, waitFor } from "@testing-library/vue";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { createPinia, setActivePinia } from "pinia";
import FileUploader from "../src/components/file-uploader/FileUploader.vue";
import { useAuthStore } from "../src/stores/auth.store";

const { uploadFileMock } = vi.hoisted(() => ({ uploadFileMock: vi.fn() }));

vi.mock("../src/services/files.service", () => ({
  SEARCH_PAGE_SIZE: 100,
  filesService: { uploadFile: uploadFileMock },
}));

function installPinia(maxFileSizeBytes = 1024) {
  const pinia = createPinia();
  setActivePinia(pinia);
  const auth = useAuthStore();
  auth.features = {
    authEnabled: true,
    multiUser: false,
    emailLogin: false,
    searchContent: false,
    shareEnabled: true,
    historyEnabled: true,
    ftpEnabled: false,
    maxFileSizeBytes,
  };
  return pinia;
}

function renderUploader(maxFileSizeBytes = 1024) {
  const pinia = installPinia(maxFileSizeBytes);
  return render(FileUploader as any, {
    global: { plugins: [pinia] },
    props: { targetPath: "" },
  });
}

function bigFile(size: number, name = "big.bin") {
  return new File([new Uint8Array(size)], name);
}

describe("FileUploader.vue upload size limit", () => {
  beforeEach(() => {
    uploadFileMock.mockReset();
    uploadFileMock.mockResolvedValue(undefined);
  });

  it("shows the server-side per-file limit", async () => {
    renderUploader(2 * 1024 * 1024);

    expect(await screen.findByText(/单文件最大 2\.0 MB/)).toBeInTheDocument();
  });

  it("marks oversized files as failed without uploading them", async () => {
    const { container } = renderUploader(1024);

    // 通过 DropZone 的文件事件把文件加入队列
    const dropZone = container.querySelector(".drop-zone")!;
    const input = container.querySelector<HTMLInputElement>(
      'input[type="file"]:not([webkitdirectory])',
    )!;
    Object.defineProperty(input, "files", {
      value: [bigFile(4096, "巨大文件.bin")],
      configurable: true,
    });
    await fireEvent.change(input, { target: { files: input.files } });

    await waitFor(() =>
      expect(
        container.querySelector(".upload-queue-row.is-error"),
      ).not.toBeNull(),
    );
    expect(container.querySelector(".upload-queue-status")?.textContent).toBe(
      "上传失败",
    );
    expect(
      container.querySelector(".upload-queue-error")?.textContent,
    ).toContain("文件过大");
    expect(dropZone).not.toBeNull();
    // 超限文件不会触发任何上传请求（前置校验）
    expect(uploadFileMock).not.toHaveBeenCalled();
  });

  it("keeps an oversized file failed when retrying", async () => {
    const { container } = renderUploader(1024);
    const input = container.querySelector<HTMLInputElement>(
      'input[type="file"]:not([webkitdirectory])',
    )!;
    Object.defineProperty(input, "files", {
      value: [bigFile(8192, "巨大文件.bin")],
      configurable: true,
    });
    await fireEvent.change(input, { target: { files: input.files } });

    await screen.findByText("重试");
    await fireEvent.click(screen.getByText("重试"));

    expect(
      container.querySelector(".upload-queue-row.is-error"),
    ).not.toBeNull();
    expect(uploadFileMock).not.toHaveBeenCalled();
  });

  it("queues files within the limit", async () => {
    const { container } = renderUploader(1024);
    const input = container.querySelector<HTMLInputElement>(
      'input[type="file"]:not([webkitdirectory])',
    )!;
    Object.defineProperty(input, "files", {
      value: [bigFile(512, "小文件.bin")],
      configurable: true,
    });
    await fireEvent.change(input, { target: { files: input.files } });

    await waitFor(() =>
      expect(container.querySelector(".upload-queue-status")?.textContent).toBe(
        "排队中",
      ),
    );
    expect(container.querySelector(".upload-queue-row.is-error")).toBeNull();
  });
});
