import { inject, provide, type InjectionKey, type Ref } from "vue";

/**
 * 文件页视图侧栏的开合控制。
 *
 * 侧栏状态（桌面收起 / 移动抽屉）仍由 `FilesLayout` 持有；文件工具栏通过
 * provide/inject 拿到同一个开关，避免把回调一路从 Home 透传到 FileBrowser。
 * 不在 `FilesLayout` 内的 FileBrowser（例如组件测试、独立嵌入）拿到 `null`，
 * 工具栏据此隐藏汉堡按钮，而不是渲染一个点了没反应的控件。
 */
export type FilesSidebarControl = {
  /** 侧栏当前是否可见：桌面 = 未收起；移动 = 抽屉已展开。 */
  isVisible: Ref<boolean>;
  /** 切换侧栏：桌面收起/展开，移动打开/关闭抽屉。 */
  toggle: () => void;
};

const FILES_SIDEBAR_CONTROL: InjectionKey<FilesSidebarControl> = Symbol(
  "vf-files-sidebar-control",
);

/** 由 `FilesLayout` 调用，向文件工具栏暴露侧栏开关。 */
export function provideFilesSidebarControl(control: FilesSidebarControl) {
  provide(FILES_SIDEBAR_CONTROL, control);
}

/** 由文件工具栏调用；没有 `FilesLayout` 时返回 `null`。 */
export function useFilesSidebarControl(): FilesSidebarControl | null {
  return inject(FILES_SIDEBAR_CONTROL, null);
}
