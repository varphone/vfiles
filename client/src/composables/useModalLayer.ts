/**
 * 模态层的背景隔离与滚动锁。
 *
 * 目的：对话框打开时，键盘/读屏/滚轮都不应继续操作背后的页面。
 * 焦点陷阱由 `Modal.vue` 负责，这里补两件 DOM 级副作用：
 *
 * 1. `inert`：把当前模态祖先链之外的兄弟节点标记为不可交互/不可聚焦；
 * 2. `overflow: hidden`：锁住 html/body 滚动。
 *
 * 多个模态叠加时以最后打开的一个为准；关闭后重新按栈顶计算，
 * 因此「对话框上再开确认框」不会把确认框自己 inert 掉。
 */

const MARK = "data-vf-modal-inert";
const SKIP_TAGS = new Set(["SCRIPT", "STYLE", "TEMPLATE", "LINK", "META"]);

const stack: HTMLElement[] = [];
let previousHtmlOverflow = "";
let previousBodyOverflow = "";
let locked = false;

/** 背景里仍须可用的区域：live region（toast）与覆盖层宿主（确认框）。 */
function isExempt(el: HTMLElement): boolean {
  if (SKIP_TAGS.has(el.tagName)) return true;
  if (el.hasAttribute("aria-live") || el.hasAttribute("aria-atomic")) {
    return true;
  }
  const role = el.getAttribute("role");
  if (role === "status" || role === "alert" || role === "log") return true;
  // 含「已激活」模态的容器是另一层对话框：留给它自己的层处理。
  // 只含隐藏模态的容器（如全局 DialogHost）可以 inert，等它打开时会重算。
  return el.querySelector(".modal.is-active") !== null;
}

function clearMarks(): void {
  for (const el of document.querySelectorAll<HTMLElement>(`[${MARK}]`)) {
    el.removeAttribute("inert");
    el.removeAttribute(MARK);
  }
}

function applyInert(): void {
  clearMarks();
  const top = stack[stack.length - 1];
  if (!top?.isConnected) return;

  let node: HTMLElement | null = top;
  while (node && node !== document.body) {
    const parent: HTMLElement | null = node.parentElement;
    if (!parent) break;
    for (const child of Array.from(parent.children)) {
      if (child === node) continue;
      const el = child as HTMLElement;
      if (isExempt(el) || el.hasAttribute("inert")) continue;
      el.setAttribute("inert", "");
      el.setAttribute(MARK, "");
    }
    node = parent;
  }
}

function lockScroll(): void {
  if (locked) return;
  locked = true;
  const html = document.documentElement;
  previousHtmlOverflow = html.style.overflow;
  previousBodyOverflow = document.body.style.overflow;
  html.style.overflow = "hidden";
  document.body.style.overflow = "hidden";
}

function unlockScroll(): void {
  if (!locked) return;
  locked = false;
  document.documentElement.style.overflow = previousHtmlOverflow;
  document.body.style.overflow = previousBodyOverflow;
}

/** 丢弃已卸载的模态（组件直接 unmount 时不会走 close 分支）。 */
function prune(): void {
  for (let index = stack.length - 1; index >= 0; index--) {
    if (!stack[index].isConnected) stack.splice(index, 1);
  }
  if (stack.length === 0) {
    clearMarks();
    unlockScroll();
  }
}

export function pushModalLayer(modal: HTMLElement): void {
  if (typeof document === "undefined") return;
  prune();
  if (stack.includes(modal)) return;
  if (stack.length === 0) lockScroll();
  stack.push(modal);
  applyInert();
}

export function popModalLayer(modal: HTMLElement): void {
  if (typeof document === "undefined") return;
  const index = stack.lastIndexOf(modal);
  if (index >= 0) stack.splice(index, 1);
  applyInert();
  if (stack.length === 0) unlockScroll();
}
