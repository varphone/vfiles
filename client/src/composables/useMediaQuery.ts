import { onBeforeUnmount, onMounted, ref } from "vue";

/**
 * 响应式媒体查询（SSR/测试环境安全）。
 *
 * 管理台在窄屏用卡片列表代替宽表格：两者结构差异大，用 CSS 双渲染会
 * 复制交互按钮与无障碍节点，所以直接按视口切换渲染分支。
 */
export function useMediaQuery(query: string) {
  const matches = ref(false);
  let media: MediaQueryList | null = null;

  const update = (event?: MediaQueryListEvent) => {
    matches.value = event ? event.matches : (media?.matches ?? false);
  };

  onMounted(() => {
    if (typeof window === "undefined" || !window.matchMedia) return;
    media = window.matchMedia(query);
    update();
    media.addEventListener?.("change", update);
  });

  onBeforeUnmount(() => {
    media?.removeEventListener?.("change", update);
    media = null;
  });

  return matches;
}
