/**
 * 网格布局的小工具。
 *
 * 方向键在网格视图里应按「一行」移动，行内元素数量取决于容器宽度与卡片最小宽度，
 * 因此需要从实际渲染结果推断列数；这里只保留可单测的纯计算部分。
 */

/**
 * 根据每张卡片相对容器的纵向偏移推断首行的列数。
 *
 * 传入的必须是**按渲染顺序**排列的卡片偏移量；遇到与首张不同的偏移即认为进入下一行。
 * 没有元素时返回 1，避免调用方把步长算成 0 而卡住。
 */
export function countRowColumns(tops: number[]): number {
  if (tops.length === 0) return 1;

  const firstTop = tops[0];
  let columns = 0;
  for (const top of tops) {
    if (Math.abs(top - firstTop) > 1) break;
    columns += 1;
  }

  return Math.max(1, columns);
}

/** 从元素的纵向偏移量里取出列数（供组件在按键时调用）。 */
export function columnsFromElements(elements: Element[]): number {
  return countRowColumns(
    elements.map((element) => (element as HTMLElement).offsetTop ?? 0),
  );
}
