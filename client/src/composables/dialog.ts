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

export interface DialogAction {
  /** 返回值：resolve 的 string 值（调用侧 switch）。 */
  value: string;
  label: string;
  /** 主行动 = accent 强调（B 决策「替换」= 主行动 ✓）。 */
  primary?: boolean;
}

export interface ChoiceDialogOptions {
  title: string;
  message: string;
  actions: DialogAction[];
}

export interface DialogRequest {
  id: number;
  kind: "confirm" | "prompt" | "choice";
  title: string;
  message: string;
  confirmText: string;
  cancelText: string;
  danger: boolean;
  defaultValue: string;
  placeholder?: string;
  /** choice 模式：N 钮动作（resolve(string)；关闭 = null ✗ A 冲突对话框三/五钮复用）。 */
  actions?: DialogAction[];
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

/** 选择/冲突对话框（A 决策三/五钮 ✗ 关闭/点遮罩 = resolve(null) = 取消语义）。 */
export function choiceDialog(options: ChoiceDialogOptions): Promise<string | null> {
  return openDialog({
    kind: "choice",
    title: options.title,
    message: options.message,
    confirmText: "",
    cancelText: "",
    danger: false,
    defaultValue: "",
    actions: options.actions,
  }) as Promise<string | null>;
}

export function resolveDialog(value: boolean | string | null): void {
  const dialog = currentDialog.value;
  if (!dialog) return;
  currentDialog.value = null;
  dialog.resolve(value);
}
