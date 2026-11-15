import type { Message } from './types';
import { sessionPreview } from './session-preview';

export interface ConversationBookmark {
  id: string;
  label: string;
}

/** 只展示已进入对话的用户消息；队列与提交占位不代表模型已经收到。 */
export function conversationBookmarks(
  messages: Message[],
): ConversationBookmark[] {
  return messages
    .filter((message) => message.role === 'user' && !message.delivery)
    .map((message) => {
      const parts = message.textParts;
      const text = parts?.length
        ? parts
            .filter((part) => part.kind !== 'files')
            .map((part) => part.text)
            .join(' ')
        : message.text;
      const files = parts?.flatMap((part) => part.files ?? []) ?? [];
      const skill = parts?.find((part) => part.kind === 'skill')?.skill;
      const label =
        sessionPreview(text, 100) ||
        (skill ? `使用 ${skill.name}` : '') ||
        (files.length
          ? `文件 × ${files.length}`
          : message.images?.length
            ? `图片 × ${message.images.length}`
            : '用户消息');
      return { id: message.id, label };
    });
}
