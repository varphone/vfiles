import type { CommitInfo } from "../../types";

export type CommitMessageMeta = {
  title: string;
  detail?: string;
  tagLabel?: string;
  tagTone?: string;
};

type CommitMessageInput = Pick<
  CommitInfo,
  "message" | "changeType" | "hasCustomMessage"
>;

const RESTORE_MESSAGE_PATTERNS = [
  /^恢复到版本\s+(.+)$/,
  /^恢复历史版本$/,
  /^从历史版本恢复$/,
  /^Restore version\s+(.+)$/i,
  /^Restore historical version$/i,
  /^Restore from history$/i,
];

function describeRestoreMessage(message: string): CommitMessageMeta | null {
  for (const pattern of RESTORE_MESSAGE_PATTERNS) {
    const match = message.match(pattern);
    if (!match) continue;

    const target = match[1]?.trim();
    return {
      title: "从历史版本恢复",
      detail: target
        ? `已基于历史版本 ${target.slice(0, 8)} 生成新版本`
        : "已基于较早版本生成新版本",
      tagLabel: "恢复",
      tagTone: "is-warning",
    };
  }

  return null;
}

function describeStructuredMessage(
  message: string,
  changeType?: CommitInfo["changeType"],
  hasCustomMessage?: boolean,
): CommitMessageMeta | null {
  if (!changeType && typeof hasCustomMessage !== "boolean") {
    return null;
  }

  if (!message) {
    return {
      title: "未填写更新消息",
      detail: "该版本没有记录备注。",
      tagLabel: "系统",
      tagTone: "is-light",
    };
  }

  switch (changeType) {
    case "added":
      return {
        title: message,
        detail: hasCustomMessage ? "创建文件并记录了更新消息" : "创建文件",
        tagLabel: "创建",
        tagTone: "is-success",
      };
    case "deleted":
      return {
        title: message,
        detail: hasCustomMessage ? "删除文件并记录了更新消息" : "删除文件",
        tagLabel: "删除",
        tagTone: "is-danger",
      };
    case "renamed":
      return {
        title: message,
        detail: hasCustomMessage ? "重命名文件并记录了更新消息" : "重命名文件",
        tagLabel: "重命名",
        tagTone: "is-link",
      };
    case "modified":
    default:
      return {
        title: message,
        detail: hasCustomMessage ? "这条更新消息会显示在版本历史里" : "系统生成的更新记录",
        tagLabel: hasCustomMessage ? "备注" : "更新",
        tagTone: hasCustomMessage ? "is-primary" : "is-info",
      };
  }
}

export function describeCommitMessage(
  commit: CommitMessageInput,
): CommitMessageMeta {
  const message = commit.message.trim();

  const restoreMeta = describeRestoreMessage(message);
  if (restoreMeta) {
    return restoreMeta;
  }

  const structured = describeStructuredMessage(
    message,
    commit.changeType,
    commit.hasCustomMessage,
  );
  if (structured) {
    return structured;
  }

  if (!message) {
    return {
      title: "未填写更新消息",
      detail: "该版本没有记录备注。",
      tagLabel: "系统",
      tagTone: "is-light",
    };
  }

  const uploadMatch = message.match(/^(?:上传|upload)\s+(.+)$/i);
  if (uploadMatch?.[1]) {
    return {
      title: uploadMatch[1].trim(),
      detail: "上传文件",
      tagLabel: "上传",
      tagTone: "is-info",
    };
  }

  if (/^(?:上传文件|upload file)$/i.test(message)) {
    return {
      title: message,
      detail: "系统生成的上传记录",
      tagLabel: "上传",
      tagTone: "is-info",
    };
  }

  return {
    title: message,
    detail: "手动填写的更新消息",
    tagLabel: "备注",
    tagTone: "is-primary",
  };
}