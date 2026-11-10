import { computed, nextTick, ref, type Ref } from "vue";
import type { FileInfo } from "../types";
import type FileUploader from "../components/file-uploader/FileUploader.vue";

export interface FileUploadDeps {
  canWrite: Ref<boolean>;
  files: Ref<FileInfo[]>;
  refresh: () => void | Promise<void>;
  notifySuccess: (message: string) => void;
}

/**
 * 上传对话框与队列的 UI 状态：打开选择器、同名集合、进度胶囊、拖放入队。
 *
 * 真正的上传由 FileUploader 组件执行；这里只保留 FileBrowser 工具栏/弹窗
 * 需要的编排与摘要。
 */
export function useFileUpload(deps: FileUploadDeps) {
  const showUploader = ref(false);
  const uploaderPick = ref<"files" | "directory" | null>(null);
  const fileUploaderRef = ref<InstanceType<typeof FileUploader> | null>(null);

  /** 目录已存在的名称：上传前同名冲突预判的数据源。 */
  const uploadExistingNames = computed(
    () => new Set(deps.files.value.map((file) => file.name)),
  );

  const uploadIndicator = computed(() => {
    const summary = fileUploaderRef.value?.summary;
    if (!summary || summary.total === 0) return null;

    const finished = summary.done + summary.failed;
    const label =
      summary.active > 0
        ? `上传中 ${finished}/${summary.total}`
        : summary.queued > 0
          ? `待上传 ${summary.total}`
          : `上传完成 ${summary.done}/${summary.total}`;
    const title =
      summary.failed > 0
        ? `上传队列：${summary.done} 成功，${summary.failed} 失败`
        : "查看上传队列";

    return { label, title };
  });

  function openUploader(pick: "files" | "directory" | null = null) {
    if (!deps.canWrite.value) return;
    uploaderPick.value = pick;
    showUploader.value = true;
  }

  function closeUploader() {
    showUploader.value = false;
    uploaderPick.value = null;
  }

  function queueDroppedFiles(files: File[]) {
    if (!deps.canWrite.value || files.length === 0) return;
    showUploader.value = true;
    void nextTick().then(() => {
      fileUploaderRef.value?.addFiles(files);
      deps.notifySuccess(`已添加 ${files.length} 个文件到上传队列`);
    });
  }

  async function handleUpload() {
    showUploader.value = false;
    deps.notifySuccess("文件上传成功");
    await deps.refresh();
  }

  return {
    showUploader,
    uploaderPick,
    fileUploaderRef,
    uploadExistingNames,
    uploadIndicator,
    openUploader,
    closeUploader,
    queueDroppedFiles,
    handleUpload,
  };
}
