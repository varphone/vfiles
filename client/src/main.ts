import { createApp } from "vue";
import { createPinia } from "pinia";
import App from "./App.vue";
import router from "./router";
import { useAuthStore } from "./stores/auth.store";

const app = createApp(App);
const pinia = createPinia();

const authStore = useAuthStore(pinia);

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

	return true;
});

if (typeof window !== "undefined") {
	window.addEventListener("vfiles:unauthorized", () => {
		if (router.currentRoute.value.name === "login") return;
		void router.push({
			name: "login",
			query: { redirect: router.currentRoute.value.fullPath },
		});
	});
}

app.use(pinia);
app.use(router);

app.mount("#app");
