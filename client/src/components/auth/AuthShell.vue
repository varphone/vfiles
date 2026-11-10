<template>
  <div class="auth-shell">
    <!-- 装饰气泡：纯 CSS 圆形 + 大半径模糊，不引外链图片，离线也能保持观感。 -->
    <div class="auth-bubbles" aria-hidden="true">
      <span class="auth-bubble auth-bubble-1"></span>
      <span class="auth-bubble auth-bubble-2"></span>
      <span class="auth-bubble auth-bubble-3"></span>
      <span class="auth-bubble auth-bubble-4"></span>
      <span class="auth-bubble auth-bubble-5"></span>
    </div>

    <div class="auth-theme">
      <ThemeToggle />
    </div>

    <main class="auth-inner">
      <!-- 卡片正上方是产品图标 + 名称，不再放副标题。 -->
      <header class="auth-brand">
        <img
          class="auth-brand-logo"
          src="/vfiles-icon.svg"
          width="56"
          height="56"
          alt=""
        />
        <h1 class="auth-brand-name">VFiles</h1>
      </header>

      <section class="auth-card" :aria-label="title || 'VFiles'">
        <h2 v-if="title" class="auth-card-title">{{ title }}</h2>
        <p v-if="description" class="auth-card-desc">{{ description }}</p>
        <slot />
      </section>
    </main>

    <footer class="auth-footer">
      <slot name="footer">
        <div class="auth-footer-row">
          <p class="auth-footer-pill">VFiles — 你自己的文件与版本历史</p>
          <a
            class="auth-record"
            href="https://beian.miit.gov.cn/"
            target="_blank"
            rel="noreferrer"
          >
            苏ICP备2026014694号-1
          </a>
        </div>
        <p class="auth-footer-meta">
          <span class="auth-version">版本 {{ appVersion }}</span>
          <span aria-hidden="true">·</span>
          <span>基于版本控制的文件管理系统</span>
        </p>
      </slot>
    </footer>
  </div>
</template>

<script setup lang="ts">
import ThemeToggle from "../common/ThemeToggle.vue";

/**
 * 认证页外壳：中性外壳 + 低饱和气泡 + 内容面板式登录卡片，与站内保持同一套配色。
 *
 * 登录 / 找回 / 重置共用；移动端卡片满宽（保留 16px 页边距）。
 * 卡片内的表单控件（浮动标签、外框输入框、深蓝主按钮、次要链接）也在这里用
 * :deep() 统一定义，三个认证页共用一套外观，避免各页样式漂移。
 */
withDefaults(
  defineProps<{
    /** 卡片标题；留空则不渲染标题。 */
    title?: string;
    description?: string;
  }>(),
  { title: "", description: "" },
);

/**
 * 页脚版本号：构建时由 Vite 从 VITE_APP_VERSION 注入；
 * 未配置时回退为产品名「VFiles」，避免页脚出现空版本号。
 */
// @ts-expect-error Vite 构建期注入的环境变量（tsconfig 未引入 vite/client 类型）
const appVersion: string = import.meta.env.VITE_APP_VERSION || "VFiles";
</script>

<style scoped>
/* 主题参数：卡片、输入框、按钮、页脚都从这里取值；
   全部指向全局 --vf-* 令牌，深浅主题由令牌自动切换。 */
.auth-shell {
  /* 认证页与站内外壳同一套面：中性外壳 + 内容面板卡片，
     主色只落在主按钮/链接/焦点上，不再用大面积高饱和蓝渐变。 */
  --auth-page-bg: var(--vf-shell-bg);
  --auth-bubble-fill: var(--vf-accent-soft);
  --auth-card-bg: var(--vf-surface);
  --auth-card-border: var(--vf-border-weak);
  --auth-card-ink: var(--vf-text);
  --auth-card-muted: var(--vf-text-muted);
  --auth-field-bg: var(--vf-surface);
  --auth-field-border: var(--vf-border);
  --auth-field-ink: var(--vf-text);
  --auth-field-placeholder: var(--vf-text-subtle);
  --auth-field-hover: var(--vf-row-hover);
  --auth-field-ring: var(--vf-focus-ring);
  --auth-primary-bg: var(--vf-accent-fill);
  --auth-primary-bg-hover: var(--vf-accent-fill-hover);
  --auth-primary-bg-active: var(--vf-accent-fill-active);
  --auth-link-ink: var(--vf-accent-text);
  --auth-footer-pill-bg: var(--vf-surface);
  --auth-footer-pill-ink: var(--vf-text-muted);
  --auth-meta-ink: var(--vf-text-muted);

  position: relative;
  display: flex;
  flex-direction: column;
  align-items: center;
  min-height: 100vh;
  min-height: 100dvh;
  padding: 2rem 1rem 1.25rem;
  overflow: hidden;
  background: var(--auth-page-bg);
  color: var(--vf-text);
}

/* 半透明气泡层：裁掉超出视口的部分，避免出现横向滚动条。 */
.auth-bubbles {
  position: absolute;
  inset: 0;
  overflow: hidden;
  pointer-events: none;
}

.auth-bubble {
  position: absolute;
  border-radius: 50%;
  background: var(--auth-bubble-fill);
  filter: blur(40px);
}

.auth-bubble-1 {
  top: -9rem;
  left: -7rem;
  width: 30rem;
  height: 30rem;
  opacity: 0.55;
}

.auth-bubble-2 {
  top: 8%;
  right: -6rem;
  width: 22rem;
  height: 22rem;
  opacity: 0.5;
}

.auth-bubble-3 {
  bottom: -9rem;
  left: 8%;
  width: 26rem;
  height: 26rem;
  opacity: 0.55;
}

.auth-bubble-4 {
  right: 6%;
  bottom: 9%;
  width: 14rem;
  height: 14rem;
  opacity: 0.45;
}

.auth-bubble-5 {
  top: 26%;
  left: 18%;
  width: 7rem;
  height: 7rem;
  opacity: 0.4;
}

/* 主题切换钮：外壳上保持内容面 + 结构线，不抢登录卡片的焦点。 */
.auth-theme {
  position: absolute;
  top: 0.9rem;
  right: 1rem;
  z-index: 3;
}

.auth-theme :deep(.theme-toggle-button) {
  border-color: var(--vf-border-weak);
  background: var(--vf-surface);
  color: var(--vf-text-muted);
  box-shadow: none;
}

.auth-theme :deep(.theme-toggle-button:hover),
.auth-theme :deep(.theme-toggle-button:focus-visible) {
  border-color: var(--vf-border);
  background: var(--vf-row-hover);
  color: var(--vf-text);
}

/* 中间内容占满剩余高度：Logo + 卡片垂直居中，页脚贴页面底部。 */
.auth-inner {
  position: relative;
  z-index: 1;
  display: flex;
  flex: 1 1 auto;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  width: 100%;
  max-width: 26rem;
  padding: 1rem 0;
}

.auth-brand {
  display: flex;
  align-items: center;
  justify-content: center;
  gap: 0.625rem;
  margin-bottom: 1.5rem;
}

.auth-brand-logo {
  display: block;
  width: 56px;
  height: 56px;
}

.auth-brand-name {
  margin: 0;
  color: var(--vf-text);
  font-size: var(--vf-text-5xl);
  font-weight: 700;
  line-height: 1;
  letter-spacing: -0.01em;
}

/* 卡片：316px、内容面、细结构线、24px 内边距（与站内内容面板同语言）。 */
.auth-card {
  box-sizing: border-box;
  width: 316px;
  max-width: 100%;
  padding: 1.5rem;
  border: 1px solid var(--auth-card-border);
  border-radius: 3px;
  background: var(--auth-card-bg);
  box-shadow: var(--vf-shadow-card);
  color: var(--auth-card-ink);
}

.auth-card-title {
  margin: 0 0 1.25rem;
  color: var(--auth-card-ink);
  font-size: var(--vf-text-2xl);
  font-weight: 700;
  line-height: 1.3;
  text-align: center;
}

.auth-card-desc {
  margin: -0.5rem 0 1.125rem;
  color: var(--auth-card-muted);
  font-size: var(--vf-text-sm);
  line-height: 1.55;
  text-align: center;
}

/* 卡片直挂的提示条单独留出间距；表单内的提示条交给 .auth-form 的 gap。 */
.auth-card > :deep(.auth-notice) {
  margin: 0 0 1rem;
}

/* ---- 卡片内共用的表单外观（浮动标签 + 外框输入框 + 深蓝主按钮） ---- */
.auth-card :deep(.auth-form) {
  display: flex;
  flex-direction: column;
  gap: 1.25rem;
}

.auth-card :deep(.auth-field) {
  position: relative;
  display: flex;
  flex-direction: column;
  min-width: 0;
}

/* Material 外框风格：标签浮在上边框，底色与卡片一致以“切开”边框。 */
.auth-card :deep(.auth-label) {
  position: absolute;
  z-index: 2;
  top: -8px;
  left: 8px;
  padding: 0 4px;
  border-radius: 2px;
  background: var(--auth-card-bg);
  color: var(--auth-card-ink);
  font-size: var(--vf-text-xs);
  font-weight: 600;
  line-height: 16px;
}

.auth-card :deep(.auth-input-wrap) {
  position: relative;
  display: flex;
  flex: 1 1 auto;
  align-items: center;
  min-width: 0;
}

.auth-card :deep(.auth-input) {
  box-sizing: border-box;
  width: 100%;
  height: 44px;
  min-height: 44px;
  padding: 10px 12px;
  border: 1px solid var(--auth-field-border);
  border-radius: 3px;
  background: var(--auth-field-bg);
  color: var(--auth-field-ink);
  font-size: var(--vf-text-lg);
  line-height: 1.2;
  box-shadow: none;
  transition:
    border-color 0.15s var(--vf-motion-standard),
    box-shadow 0.15s var(--vf-motion-standard);
}

.auth-card :deep(.auth-input::placeholder) {
  color: var(--auth-field-placeholder);
}

.auth-card :deep(.auth-input:focus) {
  border-color: var(--auth-primary-bg);
  outline: none;
  box-shadow: 0 0 0 3px var(--auth-field-ring);
}

.auth-card :deep(.auth-input:disabled) {
  opacity: 0.6;
  cursor: not-allowed;
}

.auth-card :deep(.auth-input[aria-invalid="true"]) {
  border-color: var(--vf-danger-text);
}

.auth-card :deep(.auth-input-password) {
  padding-right: 2.75rem;
}

.auth-card :deep(.auth-password-toggle) {
  position: absolute;
  right: 6px;
  z-index: 2;
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 30px;
  height: 30px;
  padding: 0;
  border: none;
  border-radius: 2px;
  background: transparent;
  color: var(--vf-text-muted);
  cursor: pointer;
}

.auth-card :deep(.auth-password-toggle:hover:not(:disabled)) {
  background: var(--auth-field-hover);
}

.auth-card :deep(.auth-password-toggle:focus-visible) {
  outline: 2px solid var(--auth-primary-bg);
  outline-offset: 1px;
}

.auth-card :deep(.auth-password-toggle:disabled) {
  opacity: 0.45;
  cursor: default;
}

/* 主按钮：整宽 44px、实心主色、右侧箭头；hover/active 逐级加深。
   .auth-primary 是主按钮的语义标记（测试/主题钩子），外观统一由 .auth-submit 提供。 */
.auth-card :deep(.auth-submit) {
  display: flex;
  align-items: center;
  justify-content: center;
  gap: 0.5rem;
  box-sizing: border-box;
  width: 100%;
  height: 44px;
  min-height: 44px;
  padding: 0 0.875rem;
  border: none;
  border-radius: 3px;
  background: var(--auth-primary-bg);
  color: var(--vf-on-accent);
  font-size: var(--vf-text-lg);
  font-weight: 600;
  line-height: 1;
  cursor: pointer;
  transition: background-color 0.15s var(--vf-motion-standard);
}

.auth-card :deep(.auth-submit:hover:not(:disabled)) {
  background: var(--auth-primary-bg-hover);
}

.auth-card :deep(.auth-submit:active:not(:disabled)) {
  background: var(--auth-primary-bg-active);
}

.auth-card :deep(.auth-submit:disabled) {
  opacity: 0.55;
  cursor: default;
}

.auth-card :deep(.auth-submit:focus-visible) {
  outline: 2px solid var(--auth-primary-bg);
  outline-offset: 2px;
}

/* 次要链接：按钮下方两行居中、13px 加粗强调色文字。 */
.auth-card :deep(.auth-links) {
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 0.125rem;
}

.auth-card :deep(.auth-link-button) {
  padding: 0;
  border: none;
  background: none;
  color: var(--auth-link-ink);
  font-size: var(--vf-text-sm);
  font-weight: 700;
  line-height: 1.8;
  text-decoration: none;
  cursor: pointer;
}

/* 非颜色区分（WCAG 1.4.1）：hover/焦点带下划线。 */
.auth-card :deep(.auth-link-button:hover),
.auth-card :deep(.auth-link-button:focus-visible) {
  text-decoration: underline;
}

.auth-card :deep(.auth-field-error) {
  margin: 6px 0 0;
  color: var(--vf-danger-text);
  font-size: var(--vf-text-xs);
  line-height: 1.4;
}

.auth-card :deep(.auth-hint) {
  margin: 0;
  color: var(--auth-card-muted);
  font-size: var(--vf-text-xs);
  line-height: 1.5;
}

/* 页脚：内容面胶囊 + 右侧低调备案号；版本号与标语放在胶囊下方一行。 */
.auth-footer {
  position: relative;
  z-index: 1;
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 0.5rem;
  padding-top: 1.25rem;
  text-align: center;
}

.auth-footer-row {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  justify-content: center;
  gap: 0.375rem 0.75rem;
}

.auth-footer-pill {
  margin: 0;
  padding: 4px 14px;
  border-radius: 999px;
  background: var(--auth-footer-pill-bg);
  box-shadow: none;
  color: var(--auth-footer-pill-ink);
  font-size: var(--vf-text-sm);
  font-weight: 500;
  line-height: 1.6;
}

.auth-footer-meta {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  justify-content: center;
  gap: 0.375rem;
  margin: 0;
  color: var(--auth-meta-ink);
  font-size: var(--vf-text-2xs);
  line-height: 1.5;
}

.auth-version {
  font-variant-numeric: tabular-nums;
}

.auth-record {
  color: var(--auth-meta-ink);
  font-size: var(--vf-text-2xs);
  text-decoration: none;
}

.auth-record:hover,
.auth-record:focus-visible {
  color: var(--vf-text);
  text-decoration: underline;
}

@media screen and (max-width: 600px) {
  .auth-shell {
    padding: 3rem 1rem 1.25rem;
  }

  .auth-inner {
    max-width: none;
    padding: 0.5rem 0;
  }

  .auth-brand {
    margin-bottom: 1.25rem;
  }

  .auth-brand-logo {
    width: 48px;
    height: 48px;
  }

  .auth-card {
    width: 100%;
    padding: 1.25rem 1.125rem;
  }
}
</style>
