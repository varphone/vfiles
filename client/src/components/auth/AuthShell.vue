<template>
  <div class="auth-shell">
    <div class="auth-theme">
      <ThemeToggle />
    </div>

    <div class="auth-inner">
      <header class="auth-brand">
        <img class="auth-brand-logo" src="/vfiles-icon.svg" alt="" />
        <h1 class="auth-brand-name">VFiles</h1>
        <p class="auth-brand-tagline">基于版本控制的文件管理系统</p>
      </header>

      <section class="auth-card" :aria-label="title">
        <h2 v-if="title" class="auth-card-title">{{ title }}</h2>
        <p v-if="description" class="auth-card-desc">{{ description }}</p>
        <slot />
      </section>

      <footer class="auth-footer">
        <slot name="footer">
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
    </div>
  </div>
</template>

<script setup lang="ts">
import ThemeToggle from "../common/ThemeToggle.vue";

/**
 * 认证页外壳：左侧品牌与产品要点，右侧表单卡片。
 *
 * 登录 / 注册 / 找回 / 重置共用；窄屏时隐藏品牌区，只保留表单。
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
/* Seafile 式登录页：居中卡片 + 品牌在上，背景为浅灰渐变。 */
.auth-shell {
  position: relative;
  display: flex;
  align-items: center;
  justify-content: center;
  min-height: 100dvh;
  padding: 2.5rem 1rem;
  background:
    radial-gradient(120% 80% at 50% -10%, rgba(37, 99, 235, 0.12), transparent 60%),
    var(--vf-canvas);
}

.auth-theme {
  position: absolute;
  top: 0.9rem;
  right: 1rem;
}

.auth-inner {
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 1.35rem;
  width: 100%;
  max-width: 25rem;
}

.auth-brand {
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 0.35rem;
  text-align: center;
}

.auth-brand-logo {
  width: 52px;
  height: 52px;
}

.auth-brand-name {
  margin: 0.15rem 0 0;
  font-size: 1.4rem;
  font-weight: 700;
  letter-spacing: -0.01em;
  color: var(--vf-text-strong);
}

.auth-brand-tagline {
  margin: 0;
  font-size: 0.8rem;
  color: var(--vf-text-muted);
}

.auth-card {
  width: 100%;
  padding: 1.5rem 1.5rem 1.35rem;
  border: 1px solid var(--vf-border-weak);
  border-radius: 10px;
  background: var(--vf-surface);
  box-shadow: 0 1px 2px rgba(16, 24, 40, 0.04), 0 12px 32px rgba(16, 24, 40, 0.06);
}

.auth-card-title {
  margin: 0 0 1rem;
  font-size: 1rem;
  font-weight: 650;
  color: var(--vf-text-strong);
}

.auth-card-desc {
  margin: -0.6rem 0 1rem;
  font-size: 0.82rem;
  line-height: 1.55;
  color: var(--vf-text-muted);
}

.auth-footer {
  font-size: 0.74rem;
  color: var(--vf-text-subtle);
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
    padding-top: 3.5rem;
  }

  .auth-card {
    padding: 1.2rem 1.1rem 1.1rem;
  }
}
</style>
