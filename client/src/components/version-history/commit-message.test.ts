import { describe, expect, it } from "vitest";

import { describeCommitMessage } from "./commit-message";

describe("describeCommitMessage", () => {
  it("maps restore messages with version ids to a restore summary", () => {
    const result = describeCommitMessage({
      message: "恢复到版本 a05ce6c8-e6e5-4b76-b37a-7eb2906b3778",
      changeType: "modified",
      hasCustomMessage: true,
    });

    expect(result).toEqual({
      title: "从历史版本恢复",
      detail: "已基于历史版本 a05ce6c8 生成新版本",
      tagLabel: "恢复",
      tagTone: "is-warning",
    });
  });

  it("maps generic restore messages to a restore summary", () => {
    const result = describeCommitMessage({
      message: "恢复历史版本",
      changeType: "modified",
      hasCustomMessage: true,
    });

    expect(result).toEqual({
      title: "从历史版本恢复",
      detail: "已基于较早版本生成新版本",
      tagLabel: "恢复",
      tagTone: "is-warning",
    });
  });

  it("keeps upload messages readable", () => {
    const result = describeCommitMessage({
      message: "上传文件",
      changeType: "added",
      hasCustomMessage: false,
    });

    expect(result).toEqual({
      title: "上传文件",
      detail: "创建文件",
      tagLabel: "创建",
      tagTone: "is-success",
    });
  });
});