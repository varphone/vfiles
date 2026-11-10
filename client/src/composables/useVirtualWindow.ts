import {
  computed,
  onBeforeUnmount,
  onMounted,
  ref,
  watch,
  type Ref,
} from "vue";

interface VirtualWindowOptions {
  /** 窗口的定位元素：其顶部用于计算页面滚动量，其高度用于判断是否有真实布局。 */
  anchor: Ref<HTMLElement | null>;
  /** 条目总数。 */
  itemCount: Ref<number>;
  /** 每行条目数（网格列数；列表为 1）。 */
  itemsPerRow: Ref<number>;
  /** 测量行高（含行间距）。没有布局时返回 0。 */
  measureRowHeight: () => number;
  /** 超过这个条目数才启用窗口化。 */
  threshold?: number;
  overscan?: number;
}

/**
 * 通用窗口化计算：把「滚动容器 / 页面滚动」两种模式统一成 start/end 与
 * 上下占位高度。列表与网格共用，组件只负责渲染 windowItems 与 padding。
 */
export function useVirtualWindow(options: VirtualWindowOptions) {
  const threshold = options.threshold ?? 60;
  const overscan = options.overscan ?? 4;

  const scrollTop = ref(0);
  const viewportHeight = ref(0);
  const rowHeight = ref(0);
  const enabled = ref(false);
  const mode = ref<"host" | "window">("host");
  const pinnedIndex = ref<number | null>(null);
  const host = ref<HTMLElement | null>(null);

  const perRow = computed(() => Math.max(1, options.itemsPerRow.value));
  const totalRows = computed(() =>
    Math.ceil(options.itemCount.value / perRow.value),
  );
  const viewportRows = computed(() =>
    rowHeight.value > 0
      ? Math.max(1, Math.ceil(viewportHeight.value / rowHeight.value))
      : 1,
  );

  function updateScrollPosition() {
    const anchor = options.anchor.value;
    if (!anchor) return;
    if (mode.value === "host" && host.value) {
      scrollTop.value = host.value.scrollTop;
    } else {
      scrollTop.value = Math.max(0, -anchor.getBoundingClientRect().top);
    }
  }

  function measure() {
    const anchor = options.anchor.value;
    if (!anchor) return;
    const scrollingHost = anchor.closest<HTMLElement>(".desktop-list-shell");
    host.value = scrollingHost;
    const hostScrolls =
      Boolean(scrollingHost) &&
      scrollingHost!.clientHeight > 0 &&
      scrollingHost!.scrollHeight > scrollingHost!.clientHeight + 1;
    mode.value = hostScrolls ? "host" : "window";
    viewportHeight.value = hostScrolls
      ? scrollingHost!.clientHeight
      : window.innerHeight;
    const measured = options.measureRowHeight();
    if (measured > 0) rowHeight.value = measured;
    enabled.value =
      anchor.clientHeight > 0 &&
      options.itemCount.value > threshold &&
      rowHeight.value > 0;
    updateScrollPosition();
  }

  const startRow = computed(() => {
    if (!enabled.value) return 0;
    const base = Math.min(
      Math.max(
        0,
        Math.floor(Math.max(0, scrollTop.value) / rowHeight.value) - overscan,
      ),
      Math.max(0, totalRows.value - 1),
    );
    const pinned = pinnedIndex.value;
    if (pinned !== null) {
      const pinnedRow = Math.floor(pinned / perRow.value);
      if (pinnedRow < base || pinnedRow >= base + viewportRows.value) {
        return Math.max(0, Math.min(pinnedRow - overscan, totalRows.value - 1));
      }
    }
    return base;
  });

  const endRow = computed(() => {
    if (!enabled.value) return totalRows.value;
    return Math.min(
      totalRows.value,
      startRow.value + viewportRows.value + overscan * 2,
    );
  });

  const start = computed(() => startRow.value * perRow.value);
  const end = computed(() =>
    Math.min(options.itemCount.value, endRow.value * perRow.value),
  );
  const padTop = computed(() =>
    enabled.value ? startRow.value * rowHeight.value : 0,
  );
  const padBottom = computed(() =>
    enabled.value ? (totalRows.value - endRow.value) * rowHeight.value : 0,
  );

  let frame = 0;
  let resizeObserver: ResizeObserver | null = null;
  let classObserver: MutationObserver | null = null;

  function onScroll() {
    if (frame) return;
    frame = requestAnimationFrame(() => {
      frame = 0;
      updateScrollPosition();
      pinnedIndex.value = null;
    });
  }

  function pin(index: number) {
    if (!enabled.value) return;
    if (index < start.value || index >= end.value) pinnedIndex.value = index;
  }

  onMounted(() => {
    const anchor = options.anchor.value;
    if (!anchor) return;
    measure();
    const scrollingHost = host.value;
    scrollingHost?.addEventListener("scroll", onScroll, { passive: true });
    window.addEventListener("scroll", onScroll, { passive: true });
    window.addEventListener("resize", measure);
    if (typeof ResizeObserver !== "undefined") {
      resizeObserver = new ResizeObserver(measure);
      resizeObserver.observe(anchor);
    }
    classObserver = new MutationObserver(measure);
    classObserver.observe(anchor, {
      attributes: true,
      attributeFilter: ["class", "style"],
    });
  });

  onBeforeUnmount(() => {
    host.value?.removeEventListener("scroll", onScroll);
    window.removeEventListener("scroll", onScroll);
    window.removeEventListener("resize", measure);
    resizeObserver?.disconnect();
    classObserver?.disconnect();
    if (frame) cancelAnimationFrame(frame);
  });

  watch(
    () => options.itemCount.value,
    () => {
      pinnedIndex.value = null;
      measure();
    },
  );

  return { start, end, padTop, padBottom, pin };
}
