<template>
  <div class="auth-shell">
    <div class="auth-shell-theme">
      <ThemeToggle />
    </div>

    <div class="auth-shell-inner">
      <header class="auth-shell-brand">
        <img class="auth-shell-logo" src="/vfiles-icon.svg" alt="VFiles" />
        <h1 class="auth-shell-title">VFiles</h1>
        <p class="auth-shell-subtitle">基于版本控制的文件管理系统</p>
      </header>

      <section class="auth-shell-card" :aria-label="title">
        <h2 v-if="title" class="auth-shell-card-title">{{ title }}</h2>
        <p v-if="description" class="auth-shell-card-desc">{{ description }}</p>
        <slot />
      </section>

      <footer class="auth-shell-footer">
        <slot name="footer">
          <a
            class="auth-shell-record"
            href="https://beian.miit.gov.cn/"
            target="_blank"
            rel="noreferrer"
          >
            苏ICP备2026014694号-1
          </a>
        </slot>
      </footer>
    </div>
  </div>
</template>

<script setup lang="ts">
import ThemeToggle from "../common/ThemeToggle.vue";

/**
 * 认证页外壳：品牌 + 卡片 + 页脚，登录 / 注册 / 找回 / 重置共用。
 *
 * 三种认证流程之前各写一套 Bulma 布局（有的有品牌、有的没有），
 * 抽成外壳后视觉与间距保持一致，主题切换也固定在右上角。
 */
withDefaults(
  defineProps<{
    /** 卡片标题；留空则不渲染标题（登录页用品牌标题即可）。 */
    title?: string;
    description?: string;
  }>(),
  { title: "", description: "" },
);
</script>

<style scoped>
.auth-shell {
  position: relative;
  display: flex;
  align-items: center;
  justify-content: center;
  min-height: 100dvh;
  padding: 3.5rem 1rem 1.5rem;
  background: var(--vf-shell-bg);
}

.auth-shell-theme {
  position: absolute;
  top: 0.75rem;
  right: 0.9rem;
}

.auth-shell-inner {
  display: flex;
  flex-direction: column;
  gap: 1.1rem;
  width: 100%;
  max-width: 26rem;
}

.auth-shell-brand {
  text-align: center;
}

.auth-shell-logo {
  width: 3rem;
  height: 3rem;
}

.auth-shell-title {
  margin: 0.5rem 0 0;
  font-size: 1.6rem;
  font-weight: 700;
  letter-spacing: 0.01em;
  color: var(--vf-text-strong);
}

.auth-shell-subtitle {
  margin: 0.2rem 0 0;
  color: var(--vf-text-muted);
  font-size: 0.85rem;
}

.auth-shell-card {
  /* 与卡片家族节奏一致（round 4） */
  padding: 1.1rem 1.2rem 1.3rem;
  border: 1px solid var(--vf-border-weak);
  border-radius: var(--vf-radius-lg);
  background: var(--vf-surface);
  box-shadow: var(--vf-shadow-card);
}

.auth-shell-card-title {
  margin: 0 0 0.2rem;
  font-size: 1.05rem;
  font-weight: 700;
  color: var(--vf-text-strong);
}

.auth-shell-card-desc {
  margin: 0 0 1rem;
  color: var(--vf-text-muted);
  font-size: 0.875rem;
  line-height: 1.5;
}

.auth-shell-footer {
  text-align: center;
  font-size: 0.75rem;
}

.auth-shell-record {
  color: var(--vf-text-muted);
}

.auth-shell-record:hover,
.auth-shell-record:focus-visible {
  color: var(--vf-accent);
  text-decoration: underline;
}

@media screen and (max-width: 480px) {
  .auth-shell {
    padding: 3rem 0.75rem 1rem;
    align-items: flex-start;
  }
}
</style>
