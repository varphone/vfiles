import { ref } from "vue";

export interface ConfirmDialogOptions {
  title?: string;
  message: string;
  confirmText?: string;
  cancelText?: string;
  danger?: boolean;
}

export interface PromptDialogOptions extends ConfirmDialogOptions {
  defaultValue?: string;
  placeholder?: string;
}

export interface DialogRequest {
  id: number;
  kind: "confirm" | "prompt";
  title: string;
  message: string;
  confirmText: string;
  cancelText: string;
  danger: boolean;
  defaultValue: string;
  placeholder?: string;
  resolve: (value: boolean | string | null) => void;
}

const currentDialog = ref<DialogRequest | null>(null);
let nextDialogId = 1;

function openDialog(
  request: Omit<DialogRequest, "id" | "resolve">,
): Promise<boolean | string | null> {
  return new Promise((resolve) => {
    currentDialog.value = {
      ...request,
      id: nextDialogId++,
      resolve,
    };
  });
}

export const dialogState = currentDialog;

export function confirmDialog(options: ConfirmDialogOptions): Promise<boolean> {
  return openDialog({
    kind: "confirm",
    title: options.title ?? "确认操作",
    message: options.message,
    confirmText: options.confirmText ?? "确定",
    cancelText: options.cancelText ?? "取消",
    danger: options.danger ?? false,
    defaultValue: "",
  }) as Promise<boolean>;
}

export function promptDialog(
  options: PromptDialogOptions,
): Promise<string | null> {
  return openDialog({
    kind: "prompt",
    title: options.title ?? "请输入",
    message: options.message,
    confirmText: options.confirmText ?? "确定",
    cancelText: options.cancelText ?? "取消",
    danger: options.danger ?? false,
    defaultValue: options.defaultValue ?? "",
    placeholder: options.placeholder,
  }) as Promise<string | null>;
}

export function resolveDialog(value: boolean | string | null): void {
  const dialog = currentDialog.value;
  if (!dialog) return;
  currentDialog.value = null;
  dialog.resolve(value);
}
