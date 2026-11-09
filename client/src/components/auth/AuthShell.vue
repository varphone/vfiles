<template>
  <div class="auth-shell">
    <div class="auth-theme">
      <ThemeToggle />
    </div>

    <main class="auth-inner">
      <!-- 品牌区在卡片上方（Nextcloud 形态）：Logo + 产品名 + 一句副标题。 -->
      <header class="auth-brand">
        <img
          class="auth-brand-logo"
          src="/vfiles-icon.svg"
          width="48"
          height="48"
          alt=""
        />
        <h1 class="auth-brand-name">VFiles</h1>
        <p class="auth-brand-tagline">基于版本控制的文件管理系统</p>
      </header>

      <section class="auth-card" :aria-label="title || 'VFiles'">
        <h2 v-if="title" class="auth-card-title">{{ title }}</h2>
        <p v-if="description" class="auth-card-desc">{{ description }}</p>
        <slot />
      </section>

      <footer class="auth-footer">
        <slot name="footer">
          <span class="auth-version">版本 {{ appVersion }}</span>
          <span aria-hidden="true">·</span>
          <a
            class="auth-record"
            href="https://beian.miit.gov.cn/"
            target="_blank"
            rel="noreferrer"
          >
            苏ICP备2026014694号-1
          </a>
        </slot>
      </footer>
    </main>
  </div>
</template>

<script setup lang="ts">
import ThemeToggle from "../common/ThemeToggle.vue";

/**
 * 认证页外壳：全屏背景 + 居中白色卡片，品牌位于卡片上方。
 *
 * 登录 / 找回 / 重置共用；移动端卡片满宽（保留 16px 页边距）。
 */
withDefaults(
  defineProps<{
    /** 卡片标题；留空则不渲染标题（登录页用品牌标题即可）。 */
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
/* Nextcloud 式登录页：全屏柔和渐变 + 极轻网格，品牌 / 卡片 / 页脚整体居中。
   不使用外链图片，离线也能保持同样的背景观感。 */
.auth-shell {
  /* 深浅色两套背景参数：渐变底色由 --vf-* 令牌跟随主题切换，
     这里只额外调整网格强度与顶部光晕，深色下更克制。 */
  --auth-grid-opacity: 0.6;
  --auth-glow: var(--vf-accent-soft);

  position: relative;
  display: flex;
  align-items: center;
  justify-content: center;
  min-height: 100vh;
  min-height: 100dvh;
  padding: 2.5rem 1rem;
  background:
    radial-gradient(90% 70% at 50% -10%, var(--auth-glow), transparent 65%),
    linear-gradient(180deg, var(--vf-surface-sunken) 0%, var(--vf-canvas) 100%);
  color: var(--vf-text);
}

/* 极轻网格：用边框令牌画 32px 细线，径向遮罩让四周自然淡出。 */
.auth-shell::before {
  content: "";
  position: absolute;
  inset: 0;
  background-image:
    linear-gradient(to right, var(--vf-border-weak) 1px, transparent 1px),
    linear-gradient(to bottom, var(--vf-border-weak) 1px, transparent 1px);
  background-size: 32px 32px;
  opacity: var(--auth-grid-opacity);
  -webkit-mask-image: radial-gradient(
    ellipse at center,
    #000 35%,
    transparent 82%
  );
  mask-image: radial-gradient(ellipse at center, #000 35%, transparent 82%);
  pointer-events: none;
}

[data-theme="dark"] .auth-shell {
  --auth-grid-opacity: 0.36;
  --auth-glow: var(--vf-accent-soft-strong);
}

/* 跟随系统深色（未显式选择浅色时）走同一套深色参数。 */
@media (prefers-color-scheme: dark) {
  :root:not([data-theme="light"]) .auth-shell {
    --auth-grid-opacity: 0.36;
    --auth-glow: var(--vf-accent-soft-strong);
  }
}

.auth-theme {
  position: absolute;
  top: 0.9rem;
  right: 1rem;
  z-index: 2;
}

.auth-inner {
  position: relative;
  z-index: 1;
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 1.5rem;
  width: 100%;
  max-width: 380px;
}

.auth-brand {
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 0.35rem;
  text-align: center;
}

.auth-brand-logo {
  width: 48px;
  height: 48px;
}

.auth-brand-name {
  margin: 0.2rem 0 0;
  font-size: 1.5rem;
  font-weight: 700;
  letter-spacing: -0.02em;
  color: var(--vf-text-strong);
}

.auth-brand-tagline {
  margin: 0;
  font-size: 0.8125rem;
  line-height: 1.5;
  color: var(--vf-text-muted);
}

.auth-card {
  width: 100%;
  padding: 1.5rem;
  border: 1px solid var(--vf-border-weak);
  border-radius: 12px;
  background: var(--vf-surface);
  box-shadow: var(--vf-shadow-card), var(--vf-shadow-md);
}

.auth-card-title {
  margin: 0 0 1rem;
  font-size: 1.0625rem;
  font-weight: 650;
  line-height: 1.4;
  color: var(--vf-text-strong);
}

.auth-card-desc {
  margin: -0.5rem 0 1.15rem;
  font-size: 0.8125rem;
  line-height: 1.55;
  color: var(--vf-text-muted);
}

.auth-footer {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  justify-content: center;
  gap: 0.4rem;
  font-size: 0.75rem;
  line-height: 1.5;
  text-align: center;
  color: var(--vf-text-subtle);
}

.auth-version {
  font-variant-numeric: tabular-nums;
}

.auth-record {
  color: var(--vf-text-subtle);
  text-decoration: none;
}

.auth-record:hover,
.auth-record:focus-visible {
  color: var(--vf-accent-text);
  text-decoration: underline;
}

@media screen and (max-width: 600px) {
  .auth-shell {
    align-items: flex-start;
    padding: 2.25rem 1rem 1.25rem;
  }

  .auth-inner {
    gap: 1.25rem;
    max-width: none;
  }

  .auth-card {
    width: 100%;
    padding: 1.25rem 1.1rem;
  }
}
</style>
