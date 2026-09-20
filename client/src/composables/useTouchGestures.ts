import { computed, ref, type Ref } from "vue";
import { useAppStore } from "../stores/app.store";

export interface TouchGestureDeps {
  /** 仅在移动端布局启用。 */
  enabled: Ref<boolean>;
  /** 弹窗打开等场景下禁用（避免手势与滚动/弹层冲突）。 */
  isBlocked: () => boolean;
  refresh: () => void | Promise<void>;
  /** 从屏幕左缘右滑时返回上一级。 */
  goBack: () => void;
}

const PULL_THRESHOLD = 60;
const PULL_MAX = 90;
const EDGE_ZONE = 24;

/**
 * 移动端手势：下拉刷新 + 左缘右滑返回。
 *
 * 从 FileBrowser 抽出；下拉达到阈值后调用 refresh 并提示成功。
 */
export function useTouchGestures(deps: TouchGestureDeps) {
  const appStore = useAppStore();

  const pullDistance = ref(0);
  const pullReady = ref(false);
  const pullRefreshing = ref(false);

  const pullIndicatorVisible = computed(
    () => pullRefreshing.value || pullDistance.value > 10,
  );

  const touchStart = ref({ x: 0, y: 0, t: 0 });
  const touchMode = ref<"none" | "pull" | "swipe">("none");

  function active(): boolean {
    return deps.enabled.value && !deps.isBlocked();
  }

  function reset() {
    pullDistance.value = 0;
    pullReady.value = false;
    touchMode.value = "none";
  }

  function onTouchStart(event: TouchEvent) {
    if (!active()) return;
    const touch = event.touches[0];
    if (!touch) return;
    touchStart.value = {
      x: touch.clientX,
      y: touch.clientY,
      t: Date.now(),
    };
    touchMode.value = "none";
  }

  function onTouchMove(event: TouchEvent) {
    if (!active()) return;
    const touch = event.touches[0];
    if (!touch) return;

    const dx = touch.clientX - touchStart.value.x;
    const dy = touch.clientY - touchStart.value.y;

    if (touchMode.value === "none") {
      if (Math.abs(dx) > 12 && Math.abs(dx) > Math.abs(dy)) {
        touchMode.value = "swipe";
      } else if (dy > 8 && Math.abs(dy) > Math.abs(dx) && window.scrollY <= 0) {
        touchMode.value = "pull";
      }
    }

    if (
      touchMode.value === "pull" &&
      window.scrollY <= 0 &&
      !pullRefreshing.value
    ) {
      const next = Math.min(PULL_MAX, Math.max(0, dy));
      pullDistance.value = next;
      pullReady.value = next >= PULL_THRESHOLD;
    }
  }

  async function onTouchEnd(event: TouchEvent) {
    if (!active()) return;

    const changed = event.changedTouches[0];
    if (!changed) {
      reset();
      return;
    }

    const dx = changed.clientX - touchStart.value.x;
    const dy = changed.clientY - touchStart.value.y;

    if (touchMode.value === "swipe") {
      const fromEdge = touchStart.value.x <= EDGE_ZONE;
      const horizontal = dx > 80 && Math.abs(dy) < 60;
      if (fromEdge && horizontal) {
        deps.goBack();
      }
    }

    if (
      touchMode.value === "pull" &&
      pullReady.value &&
      !pullRefreshing.value
    ) {
      pullRefreshing.value = true;
      try {
        await Promise.resolve(deps.refresh());
        appStore.success("已刷新");
      } finally {
        pullRefreshing.value = false;
      }
    }

    reset();
  }

  return {
    pullDistance,
    pullReady,
    pullRefreshing,
    pullIndicatorVisible,
    onTouchStart,
    onTouchMove,
    onTouchEnd,
  };
}
