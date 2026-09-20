import { beforeEach, describe, expect, it, vi } from "vitest";
import { createPinia, setActivePinia } from "pinia";
import { ref } from "vue";
import { useTouchGestures } from "../src/composables/useTouchGestures";

function touchEvent(x: number, y: number): TouchEvent {
  const point = { clientX: x, clientY: y };
  return {
    touches: [point],
    changedTouches: [point],
  } as unknown as TouchEvent;
}

function setup(overrides: { enabled?: boolean; blocked?: boolean } = {}) {
  const enabled = ref(overrides.enabled ?? true);
  const blocked = ref(overrides.blocked ?? false);
  const refresh = vi.fn(async () => {});
  const goBack = vi.fn();

  const gestures = useTouchGestures({
    enabled,
    isBlocked: () => blocked.value,
    refresh,
    goBack,
  });

  return { gestures, enabled, blocked, refresh, goBack };
}

describe("useTouchGestures", () => {
  beforeEach(() => {
    setActivePinia(createPinia());
    Object.defineProperty(window, "scrollY", {
      value: 0,
      writable: true,
      configurable: true,
    });
  });

  it("refreshes after a pull past the threshold", async () => {
    const harness = setup();

    harness.gestures.onTouchStart(touchEvent(10, 5));
    harness.gestures.onTouchMove(touchEvent(10, 80));

    expect(harness.gestures.pullReady.value).toBe(true);
    expect(harness.gestures.pullIndicatorVisible.value).toBe(true);

    await harness.gestures.onTouchEnd(touchEvent(10, 80));

    expect(harness.refresh).toHaveBeenCalledTimes(1);
    expect(harness.gestures.pullRefreshing.value).toBe(false);
    expect(harness.gestures.pullReady.value).toBe(false);
  });

  it("ignores short pulls", async () => {
    const harness = setup();

    harness.gestures.onTouchStart(touchEvent(10, 5));
    harness.gestures.onTouchMove(touchEvent(10, 30));
    expect(harness.gestures.pullReady.value).toBe(false);

    await harness.gestures.onTouchEnd(touchEvent(10, 30));
    expect(harness.refresh).not.toHaveBeenCalled();
  });

  it("goes back on a right swipe starting from the left edge", async () => {
    const harness = setup();

    harness.gestures.onTouchStart(touchEvent(10, 100));
    harness.gestures.onTouchMove(touchEvent(120, 105));
    await harness.gestures.onTouchEnd(touchEvent(120, 105));

    expect(harness.goBack).toHaveBeenCalledTimes(1);
  });

  it("does not go back when the swipe starts away from the edge", async () => {
    const harness = setup();

    harness.gestures.onTouchStart(touchEvent(80, 100));
    harness.gestures.onTouchMove(touchEvent(200, 105));
    await harness.gestures.onTouchEnd(touchEvent(200, 105));

    expect(harness.goBack).not.toHaveBeenCalled();
  });

  it("is inert when disabled or blocked", async () => {
    const disabled = setup({ enabled: false });
    disabled.gestures.onTouchStart(touchEvent(10, 5));
    disabled.gestures.onTouchMove(touchEvent(10, 90));
    await disabled.gestures.onTouchEnd(touchEvent(10, 90));
    expect(disabled.refresh).not.toHaveBeenCalled();

    const blocked = setup({ blocked: true });
    blocked.gestures.onTouchStart(touchEvent(10, 5));
    blocked.gestures.onTouchMove(touchEvent(10, 90));
    await blocked.gestures.onTouchEnd(touchEvent(10, 90));
    expect(blocked.refresh).not.toHaveBeenCalled();
  });
});
