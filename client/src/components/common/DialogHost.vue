<template>
  <Modal
    :show="dialog !== null"
    :title="dialog?.title ?? '确认操作'"
    layer="overlay"
    @close="cancel"
  >
    <template v-if="dialog">
      <p class="dialog-message">{{ dialog.message }}</p>
      <!-- choice 模式（A 冲突对话框 ✗ 动作循环钮：primary = accent 主行动） -->
      <div v-if="dialog.kind === 'choice' && dialog.actions?.length" class="dialog-actions">
        <button
          v-for="(action, index) in dialog.actions"
          :key="action.value"
          type="button"
          class="button"
          :class="{ 'is-primary': action.primary }"
          :data-autofocus="index === 0 ? '' : undefined"
          @click="choose(action.value)"
        >
          {{ action.label }}
        </button>
      </div>
      <div v-if="dialog.kind === 'prompt'" class="field mt-4">
        <div class="control">
          <input
            ref="inputRef"
            v-model="inputValue"
            class="input"
            type="text"
            :placeholder="dialog.placeholder"
            @keyup.enter="accept"
          />
        </div>
      </div>
    </template>

    <template #footer>
      <!-- 确认类对话框默认焦点 = 中性钮（r133 ✓ 防 Enter 误确认）；
           prompt 类焦点走 body 输入（自然获焦 ✓） -->
      <button
        class="button"
        type="button"
        :data-autofocus="dialog?.kind === 'confirm' ? '' : undefined"
        @click="cancel"
      >
        {{ dialog?.cancelText ?? "取消" }}
      </button>
      <button
        class="button"
        :class="dialog?.danger ? 'is-danger' : 'is-link'"
        type="button"
        @click="accept"
      >
        {{ dialog?.confirmText ?? "确定" }}
      </button>
    </template>
  </Modal>
</template>

<script setup lang="ts">
import { nextTick, ref, watch } from "vue";
import Modal from "./Modal.vue";
import {
  dialogState as dialog,
  resolveDialog,
  type DialogRequest,
} from "../../composables/dialog";

const inputValue = ref("");
function choose(value: string) {
  resolveDialog(value);
}
const inputRef = ref<HTMLInputElement | null>(null);

watch(dialog, async (request: DialogRequest | null) => {
  inputValue.value = request?.defaultValue ?? "";
  if (request?.kind === "prompt") {
    await nextTick();
    inputRef.value?.focus();
    inputRef.value?.select();
  }
});

function cancel() {
  resolveDialog(
    dialog.value?.kind === "prompt" || dialog.value?.kind === "choice"
      ? null
      : false,
  );
}

function accept() {
  if (!dialog.value) return;
  if (dialog.value.kind === "choice") return; // 动作钮自行 resolve（无表单提交）
  resolveDialog(dialog.value.kind === "prompt" ? inputValue.value : true);
}
</script>

<style scoped>
.dialog-message {
  white-space: pre-line;
  line-height: 1.6;
}
</style>
