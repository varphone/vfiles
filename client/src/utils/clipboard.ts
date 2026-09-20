/**
 * 复制文本到剪贴板。
 *
 * 优先使用异步 Clipboard API；在非安全上下文（http 访问、旧浏览器）下
 * 该 API 不可用，退回到隐藏的 `<textarea>` + `document.execCommand("copy")`。
 *
 * 返回是否复制成功，调用方据此给出反馈。
 */
export async function copyText(text: string): Promise<boolean> {
  if (!text) return false;

  if (typeof navigator !== "undefined" && navigator.clipboard?.writeText) {
    try {
      await navigator.clipboard.writeText(text);
      return true;
    } catch {
      // 权限被拒绝或非安全上下文，继续尝试回退方案
    }
  }

  if (typeof document === "undefined") return false;

  const textarea = document.createElement("textarea");
  textarea.value = text;
  textarea.setAttribute("readonly", "");
  textarea.style.position = "fixed";
  textarea.style.top = "-1000px";
  textarea.style.opacity = "0";

  try {
    document.body.appendChild(textarea);
    textarea.select();
    return document.execCommand("copy");
  } catch {
    return false;
  } finally {
    // 无论成功与否都要移除临时节点，避免残留 DOM
    textarea.remove();
  }
}
