<template>
  <!-- v-if：帮助面板是纯展示内容，关闭后不必留在 DOM 里 -->
  <Modal v-if="show" title="键盘快捷键" :show="show" @close="emit('close')">
    <div class="shortcuts">
      <p class="shortcuts-hint">
        这些快捷键在文件列表中生效（输入框或弹窗聚焦时不会触发）。按
        <kbd>?</kbd> 可随时打开本页。
      </p>

      <section
        v-for="group in groups"
        :key="group.title"
        class="shortcuts-group"
      >
        <h3 class="shortcuts-group-title">{{ group.title }}</h3>
        <dl class="shortcuts-list">
          <div
            v-for="item in group.items"
            :key="item.keys"
            class="shortcuts-row"
          >
            <dt class="shortcuts-keys">
              <kbd v-for="key in item.keys.split(' ')" :key="key">{{
                key
              }}</kbd>
            </dt>
            <dd class="shortcuts-desc">{{ item.description }}</dd>
          </div>
        </dl>
      </section>
    </div>
  </Modal>
</template>

<script setup lang="ts">
import Modal from "../common/Modal.vue";

/**
 * 键盘快捷键帮助面板。
 *
 * 主流网盘/文件管理器都有「? 查看快捷键」入口；这里集中列出列表的真实快捷键，
 * 与 `FileBrowser` 的全局 keydown 处理保持一致。
 */
defineProps<{ show: boolean }>();

const emit = defineEmits<{ (e: "close"): void }>();

const groups = [
  {
    title: "版本对比",
    items: [
      { keys: "U S", description: "切换 统一 / 并排 视图（对比打开时）" },
      {
        keys: "PageUp PageDown",
        description: "预览中 上一张 / 下一张（与 ← → 同）",
      },
      { keys: "F", description: "预览中 切换全屏（Esc 全屏中先退全屏）" },
    ],
  },
  {
    title: "导航",
    items: [
      { keys: "↑ ↓ ← →", description: "移动高亮行（网格中 ↑↓ 按整行移动）" },
      { keys: "Home End", description: "跳到列表开头 / 结尾" },
      { keys: "PageUp PageDown", description: "按整屏上下移动" },
      {
        keys: "字母 / 数字",
        description: "按键定位到名称以该前缀开头的条目（Esc 取消）",
      },
      { keys: "Enter", description: "打开文件夹，或预览文件" },
      { keys: "Esc", description: "退出批量选择 / 关闭当前面板" },
    ],
  },
  {
    title: "选择",
    items: [
      { keys: "空格", description: "选中 / 取消选中当前高亮的条目" },
      { keys: "Ctrl A", description: "全选当前视图（⌘+A 同效）" },
      { keys: "Ctrl 单击", description: "加选 / 取消单个条目" },
      { keys: "Shift 单击", description: "从上次选中项起连续选择" },
      { keys: "Shift ↑ ↓", description: "用键盘扩展选择范围" },
    ],
  },
  {
    title: "操作",
    items: [
      { keys: "F2", description: "重命名当前条目" },
      { keys: "Delete", description: "删除选中条目（Backspace 同效）" },
      { keys: "Shift F10", description: "打开当前条目的菜单" },
      { keys: "拖拽", description: "把条目拖到文件夹、目录树或面包屑上移动" },
    ],
  },
  {
    title: "预览与搜索",
    items: [
      { keys: "← →", description: "预览中切换上一个 / 下一个文件" },
      { keys: "+ - 0 R", description: "预览图片时缩放、复位、旋转" },
      { keys: "?", description: "打开 / 关闭本快捷键面板" },
    ],
  },
];
</script>

<style scoped>
.shortcuts {
  display: flex;
  flex-direction: column;
  gap: 1rem;
  min-width: min(480px, 100%);
}

.shortcuts-hint {
  margin: 0;
  color: var(--vf-text-muted);
  font-size: 0.8rem;
  line-height: 1.6;
}

.shortcuts-group-title {
  margin: 0 0 0.4rem;
  font-size: 0.78rem;
  font-weight: 600;
  letter-spacing: 0.02em;
  color: var(--vf-text-subtle);
  text-transform: uppercase;
}

.shortcuts-list {
  display: flex;
  flex-direction: column;
  gap: 0.35rem;
  margin: 0;
}

.shortcuts-row {
  display: flex;
  align-items: baseline;
  gap: 0.75rem;
}

.shortcuts-keys {
  display: flex;
  align-items: center;
  gap: 0.25rem;
  flex: 0 0 auto;
  width: 8.5rem;
}

.shortcuts-keys kbd {
  padding: 0.1rem 0.35rem;
  border: 1px solid var(--vf-border-weak);
  border-radius: var(--vf-radius-xs);
  background: var(--vf-surface-sunken);
  color: var(--vf-text-strong);
  font-family: inherit;
  font-size: 0.72rem;
  white-space: nowrap;
}

.shortcuts-desc {
  margin: 0;
  color: var(--vf-text);
  font-size: 0.82rem;
  line-height: 1.5;
}

@media screen and (max-width: 520px) {
  .shortcuts-row {
    flex-direction: column;
    gap: 0.15rem;
  }

  .shortcuts-keys {
    width: auto;
  }
}
</style>
