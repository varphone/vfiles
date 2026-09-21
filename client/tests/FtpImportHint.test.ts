import { fireEvent, render, waitFor } from "@testing-library/vue";
import { createPinia, setActivePinia } from "pinia";
import { beforeEach, describe, expect, it, vi } from "vitest";
import FtpImportHint from "../src/components/file-uploader/FtpImportHint.vue";
import FileUploader from "../src/components/file-uploader/FileUploader.vue";
import { useAuthStore } from "../src/stores/auth.store";

const { ftpInfoMock } = vi.hoisted(() => ({ ftpInfoMock: vi.fn() }));

vi.mock("../src/services/files.service", () => ({
  SEARCH_PAGE_SIZE: 100,
  filesService: {
    getFtpInfo: ftpInfoMock,
    uploadFile: vi.fn(),
    getFilesPage: vi.fn(async () => ({ items: [], total: 0 })),
  },
}));

const { copyTextMock } = vi.hoisted(() => ({
  copyTextMock: vi.fn(async () => true),
}));
vi.mock("../src/utils/clipboard", () => ({ copyText: copyTextMock }));

const enabledInfo = {
  enabled: true,
  host: "files.example.com",
  port: 2121,
  passive_ports: { start: 50000, end: 50100 },
  tls: { enabled: true, required: true },
  example_command:
    "curl --ftp-ssl -T 本地文件 ftp://files.example.com:2121/目录/",
  path_mapping: "登录后 / 即该用户的命名空间根目录",
};

/** 每个用例共享同一个 pinia 实例，组件与断言看到同一份 store。 */
function newPinia(): ReturnType<typeof createPinia> {
  const pinia = createPinia();
  setActivePinia(pinia);
  return pinia;
}

function setFeatures(ftpEnabled: boolean) {
  const auth = useAuthStore();
  auth.user = { id: "u1", username: "alice", role: "admin" } as never;
  auth.features = {
    authEnabled: true,
    multiUser: true,
    emailLogin: false,
    searchContent: false,
    shareEnabled: true,
    historyEnabled: true,
    ftpEnabled,
  };
}

describe("FtpImportHint.vue", () => {
  beforeEach(() => {
    newPinia();
    ftpInfoMock.mockReset();
    ftpInfoMock.mockResolvedValue(enabledInfo);
    copyTextMock.mockClear();
  });

  it("loads the connection info only after expanding", async () => {
    const { getByRole, findByText } = render(FtpImportHint as any, {
      global: { plugins: [createPinia()] },
    });

    expect(ftpInfoMock).not.toHaveBeenCalled();
    await fireEvent.click(getByRole("button", { name: /FTP 批量导入/ }));

    await waitFor(() => expect(ftpInfoMock).toHaveBeenCalledTimes(1));
    expect(await findByText("files.example.com")).toBeInTheDocument();
    expect(await findByText("2121")).toBeInTheDocument();
    expect(await findByText("必须使用 FTPS")).toBeInTheDocument();
  });

  it("shows the passive range, path mapping and copies the command", async () => {
    const { getByRole, findByText, container } = render(FtpImportHint as any, {
      props: { targetPath: "docs/2026" },
      global: { plugins: [createPinia()] },
    });

    await fireEvent.click(getByRole("button", { name: /FTP 批量导入/ }));
    await waitFor(() => expect(ftpInfoMock).toHaveBeenCalled());

    expect(await findByText("50000-50100")).toBeInTheDocument();
    expect(container.textContent).toContain("docs/2026");
    expect(container.textContent).toContain("curl --ftp-ssl");

    const copyButtons = container.querySelectorAll(".ftp-import-copy");
    await fireEvent.click(copyButtons[0]);

    await waitFor(() =>
      expect(copyTextMock).toHaveBeenCalledWith("files.example.com"),
    );
  });

  it("surfaces a readable error when the info cannot be loaded", async () => {
    ftpInfoMock.mockRejectedValue(new Error("网络不可用"));
    const { getByRole, findByText } = render(FtpImportHint as any, {
      global: { plugins: [createPinia()] },
    });

    await fireEvent.click(getByRole("button", { name: /FTP 批量导入/ }));

    expect(await findByText("网络不可用")).toBeInTheDocument();
  });
});

describe("FileUploader.vue FTP entry", () => {
  beforeEach(() => {
    newPinia();
    ftpInfoMock.mockReset();
    ftpInfoMock.mockResolvedValue(enabledInfo);
  });

  it("renders the hint only when the feature flag is on", async () => {
    const pinia = newPinia();
    setFeatures(true);
    const { container } = render(FileUploader as any, {
      props: { targetPath: "" },
      global: { plugins: [pinia] },
    });
    await waitFor(() =>
      expect(container.querySelector(".ftp-import-hint")).not.toBeNull(),
    );
  });

  it("hides the hint when the feature flag is off", () => {
    const pinia = newPinia();
    setFeatures(false);
    const { container } = render(FileUploader as any, {
      props: { targetPath: "" },
      global: { plugins: [pinia] },
    });

    expect(container.querySelector(".ftp-import-hint")).toBeNull();
  });
});
