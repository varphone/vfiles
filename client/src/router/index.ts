import { nextTick } from "vue";
import { createRouter, createWebHistory } from "vue-router";

const router = createRouter({
  history: createWebHistory(),
  routes: [
    {
      path: "/",
      name: "home",
      component: () => import("../views/Home.vue"),
    },
    {
      path: "/recent",
      meta: { title: "最近" },
      name: "recent",
      component: () => import("../views/files/RecentView.vue"),
    },
    {
      path: "/favorites",
      meta: { title: "收藏" },
      name: "favorites",
      component: () => import("../views/files/FavoritesView.vue"),
    },
    {
      path: "/activity",
      meta: { title: "活动" },
      name: "activity",
      component: () => import("../views/ActivityView.vue"),
    },
    {
      path: "/storage",
      meta: { title: "空间分析" },
      name: "storage-analysis",
      component: () => import("../views/StorageAnalysis.vue"),
    },
    {
      path: "/login",
      meta: { title: "登录" },
      name: "login",
      component: () => import("../views/Login.vue"),
    },
    {
      path: "/forgot-password",
      meta: { title: "找回密码" },
      name: "forgot-password",
      component: () => import("../views/ForgotPassword.vue"),
    },
    {
      path: "/reset-password",
      meta: { title: "重置密码" },
      name: "reset-password",
      component: () => import("../views/ResetPassword.vue"),
    },
    {
      path: "/admin",
      meta: { title: "管理概览" },
      name: "admin-overview",
      component: () => import("../views/AdminOverview.vue"),
    },
    {
      path: "/admin/audit",
      meta: { title: "审计日志" },
      name: "audit-logs",
      component: () => import("../views/AuditLogs.vue"),
    },
    {
      path: "/shares",
      meta: { title: "我的分享" },
      name: "shared-links",
      component: () => import("../views/SharedLinks.vue"),
    },
    {
      path: "/settings/tokens",
      meta: { title: "访问令牌" },
      name: "access-tokens",
      component: () => import("../views/AccessTokens.vue"),
    },
    {
      path: "/settings/passwords",
      meta: { title: "访问密码" },
      name: "access-passwords",
      component: () => import("../views/AccessPasswords.vue"),
    },
    {
      path: "/admin/system",
      meta: { title: "系统" },
      name: "admin-system",
      component: () => import("../views/SystemInfo.vue"),
    },
    {
      path: "/system-info",
      redirect: "/admin/system",
    },
    {
      path: "/admin/users",
      meta: { title: "用户管理" },
      name: "admin-users",
      component: () => import("../views/AdminUsers.vue"),
    },
  ],
});

// 动态页面标题（r147 ✓ W3C 页面标题标准：页面唯一可辨标题）
// 路由切换后把焦点交给主内容：SPA 不会像整页导航那样自动重定位焦点，
// 读屏用户否则仍停在已卸载的链接上（activeElement 回落为 body）。
router.afterEach(async (to, from) => {
  const base = "VFiles";
  document.title = to.meta?.title
    ? `${to.meta.title} - ${base}`
    : `${base} - 文件管理系统`;

  if (!from.name || from.name === to.name) return;
  await nextTick();
  const active = document.activeElement;
  if (active && active !== document.body) return;
  const main = document.querySelector<HTMLElement>("main");
  if (!main) return;
  main.setAttribute("tabindex", "-1");
  main.focus({ preventScroll: true });
  main.addEventListener(
    "blur",
    () => main.removeAttribute("tabindex"),
    { once: true },
  );
});

router.addRoute({
  path: "/:catchAll(.*)",
  name: "not-found",
  meta: { title: "页面不存在" },
  component: () => import("../views/NotFound.vue"),
});

export default router;
