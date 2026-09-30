import { onBeforeUnmount } from "vue";

/**
 * 在当前组件生命周期内监听 document 事件，并在卸载前移除监听器。
 * SSR 环境没有 document 时保持空操作。
 */
export function useDocumentEventListener(
  type: string,
  listener: EventListenerOrEventListenerObject,
  options?: boolean | AddEventListenerOptions,
): void {
  if (typeof document === "undefined") return;

  const target = document;
  target.addEventListener(type, listener, options);
  onBeforeUnmount(() => target.removeEventListener(type, listener, options));
}
