import { createApp } from "vue";
import { createPinia } from "pinia";
import App from "./App.vue";
import router from "./router";
import { useAuthStore } from "./stores/auth.store";
import { useAppStore } from "./stores/app.store";

const app = createApp(App);
const pinia = createPinia();

const authStore = useAuthStore(pinia);
const appStore = useAppStore(pinia);

router.beforeEach(async (to) => {
	if (!authStore.initialized) {
		await authStore.fetchMe();
	}

	// 未启用认证：不做拦截
	if (!authStore.enabled) {
		if (to.name === "login") return { name: "home" };
		return true;
	}

	// 已登录：不允许再进登录页
	if (to.name === "login" && authStore.user) {
		const redirect = typeof to.query.redirect === "string" ? to.query.redirect : "/";
		return redirect;
	}

	// 需要登录
	if (to.name !== "login" && !authStore.user) {
		return { name: "login", query: { redirect: to.fullPath } };
	}

	// 管理页：仅 admin
	if (to.name === "admin-users" && authStore.user?.role !== "admin") {
		return { name: "home" };
	}

	return true;
});

if (typeof window !== "undefined") {
	let lastUnauthorizedAt = 0;
	window.addEventListener("vfiles:unauthorized", () => {
		if (router.currentRoute.value.name === "login") return;
		const now = Date.now();
		if (now - lastUnauthorizedAt > 1500) {
			appStore.warning("登录已过期，请重新登录");
			lastUnauthorizedAt = now;
		}
		void router.push({
			name: "login",
			query: { redirect: router.currentRoute.value.fullPath, reason: "expired" },
		});
	});
}

app.use(pinia);
app.use(router);

app.mount("#app");
