<template>
  <div class="tokens-page">
    <div class="tokens-card vf-page-card">
      <header class="tokens-header">
        <div class="tokens-header-titles">
          <h1 class="tokens-title vf-page-title">访问令牌</h1>
          <p class="tokens-subtitle vf-page-subtitle">
            给 CLI、构建系统等程序使用的 API 凭证，用
            <code>Authorization: Bearer &lt;token&gt;</code> 调用接口
          </p>
        </div>

        <div class="tokens-header-actions">
          <button
            class="vf-ghost-button is-primary"
            type="button"
            @click="openCreate"
          >
            <IconPlus :size="16" />
            <span>新建令牌</span>
          </button>
          <button
            class="vf-ghost-button"
            type="button"
            :disabled="loading"
            @click="reload"
          >
            <IconRefresh :size="16" />
            <span>刷新</span>
          </button>
          <button class="vf-ghost-button" type="button" @click="goFiles">
            <IconFolder :size="16" />
            <span>返回文件</span>
          </button>
        </div>
      </header>

      <p class="tokens-note">
        <IconShieldLock :size="14" />
        <span>
          令牌只在创建时显示一次，请立即复制保存；泄露后请立刻撤销。令牌无法创建或撤销令牌。
        </span>
      </p>

      <EmptyState
        v-if="error"
        :icon="IconAlertCircle"
        tone="error"
        title="加载失败"
        :hint="error"
      >
        <template #actions>
          <button class="vf-ghost-button is-primary" @click="reload">
            <IconRefresh :size="16" />
            <span>重试</span>
          </button>
        </template>
      </EmptyState>

      <SkeletonList
        v-else-if="loading && tokens.length === 0"
        :rows="3"
        label="加载访问令牌"
      />

      <EmptyState
        v-else-if="tokens.length === 0"
        :icon="IconKey"
        title="还没有访问令牌"
        hint="新建一个令牌，就能在脚本或 CI 里上传、下载文件"
      >
        <template #actions>
          <button class="vf-ghost-button is-primary" @click="openCreate">
            <IconPlus :size="16" />
            <span>新建令牌</span>
          </button>
        </template>
      </EmptyState>

      <div v-else class="tokens-table-wrap">
        <table class="tokens-table">
          <thead>
            <tr>
              <th scope="col">名称</th>
              <th scope="col">令牌</th>
              <th scope="col">创建时间</th>
              <th scope="col">最近使用</th>
              <th scope="col">有效期</th>
              <th scope="col">状态</th>
              <th scope="col" class="tokens-actions-head">操作</th>
            </tr>
          </thead>
          <tbody>
            <tr v-for="token in tokens" :key="token.id">
              <td class="tokens-name">{{ token.name }}</td>
              <td>
                <code class="tokens-prefix">{{ token.token_prefix }}…</code>
              </td>
              <td class="tokens-muted">
                {{ formatRelativeDate(token.created_at) }}
              </td>
              <td class="tokens-muted">
                {{
                  token.last_used_at
                    ? formatRelativeDate(token.last_used_at)
                    : "从未使用"
                }}
              </td>
              <td class="tokens-muted">{{ expiryLabel(token) }}</td>
              <td>
                <span
                  class="tokens-status vf-status-pill"
                  :class="statusClass(token)"
                >
                  {{ statusLabel(token) }}
                </span>
              </td>
              <td class="tokens-actions">
                <button
                  v-if="!token.revoked_at"
                  class="vf-ghost-button tokens-revoke"
                  type="button"
                  @click="revoke(token)"
                >
                  <IconTrash :size="14" />
                  <span>撤销</span>
                </button>
                <span v-else class="tokens-muted">已撤销</span>
              </td>
            </tr>
          </tbody>
        </table>
      </div>

      <section v-if="tokens.length > 0" class="tokens-usage">
        <h2 class="tokens-usage-title">用法示例</h2>
        <pre class="tokens-usage-code"><code>{{ usageSnippet }}</code></pre>
        <button class="vf-ghost-button" type="button" @click="copySnippet">
          <IconCopy :size="14" />
          <span>复制示例</span>
        </button>
      </section>
    </div>

    <!-- 新建令牌 -->
    <Modal :show="createOpen" title="新建访问令牌" @close="closeCreate">
      <div class="tokens-form">
        <label class="tokens-label" for="token-name">名称</label>
        <!-- 字段级 aria 链（r68 约定兑现示范 ✓ 行内错误 ↔ 控件关联）：
             aria-invalid 标错 + aria-describedby 指错误文案 id -->
        <input
          id="token-name"
          autocomplete="off"
          aria-required="true"
          v-model.trim="form.name"
          class="input is-small"
          type="text"
          maxlength="64"
          placeholder="例如：CI 构建、备份脚本"
          :aria-invalid="createError ? 'true' : undefined"
          :aria-describedby="createError ? 'token-name-error' : undefined"
          @keyup.enter="submitCreate"
        />

        <label class="tokens-label" for="token-expiry">有效期</label>
        <div class="select is-small">
          <select id="token-expiry" v-model.number="form.expiresInDays">
            <option
              v-for="option in expiryOptions"
              :key="option.days"
              :value="option.days"
            >
              {{ option.label }}
            </option>
          </select>
        </div>

        <p
          id="token-name-error"
          v-if="createError"
          class="tokens-error"
          role="alert"
        >
          {{ createError }}
        </p>
      </div>

      <template #footer>
        <button class="vf-ghost-button" @click="closeCreate">取消</button>
        <button
          class="vf-ghost-button is-primary"
          type="button"
          :disabled="creating || !form.name"
          @click="submitCreate"
        >
          <IconKey :size="16" />
          <span>{{ creating ? "创建中…" : "创建" }}</span>
        </button>
      </template>
    </Modal>

    <!-- 明文只显示这一次 -->
    <Modal
      :show="plaintext !== ''"
      title="令牌已创建"
      @close="dismissPlaintext"
    >
      <div class="tokens-created">
        <p class="tokens-created-hint">
          <IconAlertTriangle :size="16" />
          <span>这是唯一一次显示明文，关闭后就看不到了。</span>
        </p>
        <div class="tokens-created-row">
          <input
            ref="plaintextInput"
            class="input is-small tokens-created-input"
            type="text"
            :value="plaintext"
            readonly
            aria-label="新建的访问令牌"
            @focus="selectPlaintext"
          />
          <button
            class="vf-ghost-button is-primary"
            type="button"
            @click="copyPlaintext"
          >
            <IconCopy :size="16" />
            <span>{{ copied ? "已复制" : "复制" }}</span>
          </button>
        </div>
      </div>

      <template #footer>
        <button class="vf-ghost-button" @click="dismissPlaintext">
          我已保存
        </button>
      </template>
    </Modal>
  </div>
</template>

<script setup lang="ts">
import { computed, onMounted, ref } from "vue";
import { useRouter } from "vue-router";
import {
  IconAlertCircle,
  IconAlertTriangle,
  IconCopy,
  IconFolder,
  IconKey,
  IconPlus,
  IconRefresh,
  IconShieldLock,
  IconTrash,
} from "@tabler/icons-vue";
import EmptyState from "../components/common/EmptyState.vue";
import Modal from "../components/common/Modal.vue";
import SkeletonList from "../components/common/SkeletonList.vue";
import { confirmDialog } from "../composables/dialog";
import { useAppStore } from "../stores/app.store";
import { filesService } from "../services/files.service";
import { copyText } from "../utils/clipboard";
import { formatRelativeDate } from "../utils/filePresentation";
import type { AccessToken, TokenExpiryOption } from "../types";

/**
 * 访问令牌管理页。
 *
 * 明文令牌只在创建响应里出现一次，这里用一个独立对话框展示并提供复制。
 */
const app = useAppStore();
const router = useRouter();

const tokens = ref<AccessToken[]>([]);
const expiryOptions = ref<TokenExpiryOption[]>([{ days: 0, label: "永久" }]);
const loading = ref(false);
const error = ref("");

const createOpen = ref(false);
const creating = ref(false);
const createError = ref("");
const form = ref({ name: "", expiresInDays: 0 });

const plaintext = ref("");
const copied = ref(false);
const plaintextInput = ref<HTMLInputElement | null>(null);

const usageSnippet = computed(
  () => `# 上传（单请求，最简单）
curl -T app.tar.gz \
  -H "Authorization: Bearer $VFILES_TOKEN" \
  "$VFILES/api/files/upload/ci/app.tar.gz"

# 上传到指定目录（URL 以 / 结尾时，curl 自动补本地文件名）
curl -T app.tar.gz \
  -H "Authorization: Bearer $VFILES_TOKEN" \
  "$VFILES/api/files/upload/ci/"

# 下载
curl -sS -H "Authorization: Bearer $VFILES_TOKEN" \
  "$VFILES/api/files/content?path=ci/app.tar.gz" -o app.tar.gz

# 列表
curl -sS -H "Authorization: Bearer $VFILES_TOKEN" "$VFILES/api/files/tree?path=ci"`,
);

async function reload() {
  loading.value = true;
  error.value = "";
  try {
    tokens.value = await filesService.listAccessTokens();
  } catch (e) {
    error.value = e instanceof Error ? e.message : "加载访问令牌失败";
  } finally {
    loading.value = false;
  }
}

function goFiles() {
  void router.push({ name: "home" });
}

function openCreate() {
  form.value = {
    name: "",
    expiresInDays: expiryOptions.value[0]?.days ?? 0,
  };
  createError.value = "";
  creating.value = false;
  createOpen.value = true;
}

function closeCreate() {
  createOpen.value = false;
}

async function submitCreate() {
  // 空名 = 行内错误（r86 ✗✓ 原为静默 return = 点「创建」零反馈）+ aria 链联动
  if (!form.value.name) {
    createError.value = "请输入令牌名称";
    return;
  }
  creating.value = true;
  createError.value = "";
  try {
    const created = await filesService.createAccessToken(
      form.value.name,
      form.value.expiresInDays,
    );
    plaintext.value = created.plaintext;
    copied.value = false;
    createOpen.value = false;
    await reload();
    app.success(`已创建令牌「${created.token?.name ?? form.value.name}」`);
  } catch (e) {
    createError.value = e instanceof Error ? e.message : "创建失败";
  } finally {
    creating.value = false;
  }
}

function dismissPlaintext() {
  plaintext.value = "";
  copied.value = false;
}

function selectPlaintext() {
  plaintextInput.value?.select();
}

async function copyPlaintext() {
  const ok = await copyText(plaintext.value);
  copied.value = ok;
  if (ok) app.success("令牌已复制到剪贴板");
}

async function copySnippet() {
  const ok = await copyText(usageSnippet.value);
  if (ok) app.success("用法示例已复制");
}

async function revoke(token: AccessToken) {
  const ok = await confirmDialog({
    title: "撤销访问令牌",
    message: `确定撤销「${token.name}」吗？\n使用该令牌的程序会立即失去访问权限。`,
    confirmText: "撤销",
    danger: true,
  });
  if (!ok) return;

  try {
    await filesService.revokeAccessToken(token.id);
    app.success(`已撤销令牌「${token.name}」`);
    await reload();
  } catch (e) {
    app.error(e instanceof Error ? e.message : "撤销失败");
  }
}

function expiryLabel(token: AccessToken): string {
  if (!token.expires_at) return "永久";
  const remaining = new Date(token.expires_at).getTime() - Date.now();
  if (remaining <= 0)
    return `已过期（${formatRelativeDate(token.expires_at)}）`;
  return `至 ${formatRelativeDate(token.expires_at)}`;
}

function statusLabel(token: AccessToken): string {
  if (token.revoked_at) return "已撤销";
  if (!token.active) return "已过期";
  return "有效";
}

function statusClass(token: AccessToken): string {
  if (token.revoked_at) return "is-revoked";
  return token.active ? "is-active" : "is-expired";
}

onMounted(async () => {
  try {
    expiryOptions.value = await filesService.listTokenExpiryOptions();
  } catch {
    // 保底：服务端拿不到时仍提供「永久」
  }
  await reload();
});
</script>

<style scoped>
.tokens-page {
  display: flex;
  justify-content: center;
  padding: 1.25rem;
}

.tokens-header {
  display: flex;
  align-items: flex-start;
  justify-content: space-between;
  gap: 1rem;
  flex-wrap: wrap;
  padding-bottom: 0.75rem;
  border-bottom: 1px solid var(--vf-border-weak);
}

.tokens-subtitle code {
  padding: 0 0.25rem;
  border-radius: var(--vf-radius-xs);
  background: var(--vf-surface-sunken);
  font-size: 0.8rem;
}

.tokens-header-actions {
  display: flex;
  align-items: center;
  gap: 0.35rem;
}

.tokens-note {
  display: flex;
  align-items: center;
  gap: 0.35rem;
  margin: 0.75rem 0 0.25rem;
  color: var(--vf-text-muted);
  font-size: 0.8rem;
}

.tokens-table-wrap {
  margin-top: 0.6rem;
  overflow-x: auto;
}

.tokens-table {
  width: 100%;
  border-collapse: collapse;
  font-size: 0.875rem;
}

.tokens-table th {
  padding: 0.45rem 0.5rem;
  border-bottom: 1px solid var(--vf-border-weak);
  color: var(--vf-text-subtle);
  font-size: 0.75rem;
  font-weight: 600;
  text-align: left;
  white-space: nowrap;
}

.tokens-table td {
  padding: 0.5rem;
  border-bottom: 1px solid var(--vf-border-weak);
  vertical-align: middle;
}

.tokens-name {
  color: var(--vf-text-strong);
  font-weight: 600;
}

.tokens-prefix {
  padding: 0.05rem 0.3rem;
  border-radius: var(--vf-radius-xs);
  background: var(--vf-surface-sunken);
  font-size: 0.8rem;
}

.tokens-muted {
  color: var(--vf-text-subtle);
}

.tokens-status.is-active {
  background: var(--vf-success-soft);
  color: var(--vf-success-text);
}

.tokens-status.is-expired {
  background: var(--vf-warning-soft);
  color: var(--vf-warning-text);
}

.tokens-status.is-revoked {
  background: var(--vf-surface-sunken);
  color: var(--vf-text-subtle);
}

.tokens-actions-head,
.tokens-actions {
  text-align: right;
}

.tokens-revoke {
  color: var(--vf-danger-text);
}

.tokens-usage {
  margin-top: 1.1rem;
}

.tokens-usage-title {
  margin: 0 0 0.35rem;
  font-size: 0.875rem;
  font-weight: 600;
  color: var(--vf-text-strong);
}

.tokens-usage-code {
  margin: 0 0 0.5rem;
  padding: 0.6rem 0.7rem;
  border: 1px solid var(--vf-border-weak);
  border-radius: var(--vf-radius-sm);
  background: var(--vf-surface-sunken);
  color: var(--vf-text);
  font-size: 0.8rem;
  line-height: 1.5;
  overflow-x: auto;
  white-space: pre;
}

.tokens-form {
  display: flex;
  flex-direction: column;
  gap: 0.35rem;
  min-width: min(420px, 100%);
}

.tokens-label {
  color: var(--vf-text-muted);
  font-size: 0.8rem;
  font-weight: 600;
}

.tokens-error {
  margin: 0.2rem 0 0;
  color: var(--vf-danger-text);
  font-size: 0.8rem;
}

.tokens-created {
  display: flex;
  flex-direction: column;
  gap: 0.6rem;
  min-width: min(460px, 100%);
}

.tokens-created-hint {
  display: flex;
  align-items: center;
  gap: 0.35rem;
  margin: 0;
  color: var(--vf-warning-text);
  font-size: 0.8rem;
}

.tokens-created-row {
  display: flex;
  align-items: center;
  gap: 0.4rem;
}

.tokens-created-input {
  font-family: var(--vf-font-mono, monospace);
  font-size: 0.8rem;
}
</style>
