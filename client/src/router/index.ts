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
      path: "/login",
      name: "login",
      component: () => import("../views/Login.vue"),
    },
    {
      path: "/forgot-password",
      name: "forgot-password",
      component: () => import("../views/ForgotPassword.vue"),
    },
    {
      path: "/reset-password",
      name: "reset-password",
      component: () => import("../views/ResetPassword.vue"),
    },
    {
      path: "/admin/audit",
      meta: { title: "审计日志" },
      name: "audit-logs",
      component: () => import("../views/AuditLogs.vue"),
    },
    {
      path: "/shares",
      name: "shared-links",
      component: () => import("../views/SharedLinks.vue"),
    },
    {
      path: "/settings/tokens",
      name: "access-tokens",
      component: () => import("../views/AccessTokens.vue"),
    },
    {
      path: "/system-info",
      meta: { title: "系统信息" },
      name: "system-info",
      component: () => import("../views/SystemInfo.vue"),
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
router.afterEach((to) => {
  const base = "VFiles";
  document.title = to.meta?.title ? `${to.meta.title} - ${base}` : `${base} - 文件管理系统`;
});

export default router;
