<template>
  <div class="auth-layout">
    <aside class="auth-brand-panel">
      <div class="auth-brand-top">
        <img class="auth-brand-logo" src="/vfiles-icon.svg" alt="" />
        <span class="auth-brand-name">VFiles</span>
      </div>

      <div class="auth-brand-copy">
        <h1>安全、可追溯的文件协作</h1>
        <p>
          为团队提供版本历史、目录范围权限与多协议访问，
          文件和每一次变更都留在自己的服务器上。
        </p>
        <ul class="auth-features">
          <li>
            <IconHistory :size="16" />
            <span>每次变更都有版本，可随时回溯</span>
          </li>
          <li>
            <IconShieldLock :size="16" />
            <span>同一账号可用不同密码限定可见目录</span>
          </li>
          <li>
            <IconCloudUpload :size="16" />
            <span>Web、WebDAV、S3 与 FTP 互通</span>
          </li>
        </ul>
      </div>

      <a
        class="auth-record"
        href="https://beian.miit.gov.cn/"
        target="_blank"
        rel="noreferrer"
      >
        苏ICP备2026014694号-1
      </a>
    </aside>

    <div class="auth-form-panel">
      <div class="auth-theme">
        <ThemeToggle />
      </div>

      <div class="auth-form-inner">
        <header class="auth-mobile-brand">
          <img src="/vfiles-icon.svg" alt="" width="36" height="36" />
          <span>VFiles 文件管理</span>
        </header>

        <section class="auth-card" :aria-label="title">
          <h2 v-if="title" class="auth-card-title">{{ title }}</h2>
          <p v-if="description" class="auth-card-desc">{{ description }}</p>
          <slot />
        </section>

        <footer class="auth-form-footer">
          <slot name="footer">
            <span>基于版本控制的文件管理系统</span>
          </slot>
        </footer>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { IconCloudUpload, IconHistory, IconShieldLock } from "@tabler/icons-vue";
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
.auth-layout {
  display: grid;
  grid-template-columns: minmax(0, 1.05fr) minmax(0, 0.95fr);
  min-height: 100dvh;
  background: var(--vf-canvas);
}

.auth-brand-panel {
  position: relative;
  display: flex;
  flex-direction: column;
  justify-content: space-between;
  gap: 2rem;
  padding: 2.5rem 3rem;
  overflow: hidden;
  background:
    radial-gradient(120% 90% at 0% 0%, rgba(59, 130, 246, 0.28), transparent 55%),
    radial-gradient(100% 80% at 100% 100%, rgba(14, 165, 233, 0.2), transparent 60%),
    linear-gradient(150deg, #0f172a 0%, #111c33 55%, #0b1220 100%);
  color: #e6ecf7;
}

.auth-brand-panel::after {
  content: "";
  position: absolute;
  inset: 0;
  background-image:
    linear-gradient(rgba(255, 255, 255, 0.045) 1px, transparent 1px),
    linear-gradient(90deg, rgba(255, 255, 255, 0.045) 1px, transparent 1px);
  background-size: 44px 44px;
  mask-image: linear-gradient(180deg, rgba(0, 0, 0, 0.5), transparent 75%);
  pointer-events: none;
}

.auth-brand-top {
  position: relative;
  z-index: 1;
  display: flex;
  align-items: center;
  gap: 0.6rem;
}

.auth-brand-logo {
  width: 34px;
  height: 34px;
}

.auth-brand-name {
  font-size: 1.02rem;
  font-weight: 650;
  letter-spacing: 0.01em;
}

.auth-brand-copy {
  position: relative;
  z-index: 1;
  max-width: 30rem;
}

.auth-brand-copy h1 {
  margin: 0 0 0.75rem;
  font-size: clamp(1.6rem, 2.4vw, 2.15rem);
  line-height: 1.22;
  font-weight: 700;
  letter-spacing: -0.015em;
}

.auth-brand-copy > p {
  margin: 0;
  color: rgba(230, 236, 247, 0.72);
  font-size: 0.9rem;
  line-height: 1.65;
}

.auth-features {
  display: flex;
  flex-direction: column;
  gap: 0.6rem;
  margin: 1.5rem 0 0;
  padding: 0;
  list-style: none;
}

.auth-features li {
  display: flex;
  align-items: center;
  gap: 0.55rem;
  font-size: 0.83rem;
  color: rgba(230, 236, 247, 0.86);
}

.auth-features svg {
  flex: none;
  color: #7db4ff;
}

.auth-record {
  position: relative;
  z-index: 1;
  font-size: 0.72rem;
  color: rgba(230, 236, 247, 0.5);
  text-decoration: none;
}

.auth-record:hover {
  color: rgba(230, 236, 247, 0.8);
}

.auth-form-panel {
  position: relative;
  display: flex;
  align-items: center;
  justify-content: center;
  padding: 3rem 2rem;
  background: var(--vf-surface);
}

.auth-theme {
  position: absolute;
  top: 1rem;
  right: 1.1rem;
}

.auth-form-inner {
  display: flex;
  flex-direction: column;
  gap: 1.1rem;
  width: 100%;
  max-width: 24rem;
}

.auth-mobile-brand {
  display: none;
  align-items: center;
  justify-content: center;
  gap: 0.55rem;
  font-weight: 700;
  font-size: 1.1rem;
  color: var(--vf-text-strong);
}

.auth-card {
  padding: 1.35rem 1.4rem 1.5rem;
  border: 1px solid var(--vf-border-weak);
  border-radius: var(--vf-radius-lg);
  background: var(--vf-surface);
  box-shadow: var(--vf-shadow-card);
}

.auth-card-title {
  margin: 0 0 0.2rem;
  font-size: 1.15rem;
  font-weight: 700;
  color: var(--vf-text-strong);
}

.auth-card-desc {
  margin: 0 0 1rem;
  color: var(--vf-text-muted);
  font-size: 0.85rem;
  line-height: 1.55;
}

.auth-form-footer {
  text-align: center;
  font-size: 0.75rem;
  color: var(--vf-text-subtle);
}

@media screen and (max-width: 900px) {
  .auth-layout {
    grid-template-columns: minmax(0, 1fr);
  }

  .auth-brand-panel {
    display: none;
  }

  .auth-form-panel {
    padding: 4rem 1rem 2rem;
    background: var(--vf-canvas);
  }

  .auth-mobile-brand {
    display: flex;
  }

  .auth-card {
    background: var(--vf-surface);
  }
}
</style>
