import { expect, it } from 'vitest';
import { conversationBookmarks } from './conversation-bookmarks';
import type { Message } from './types';

function message(id: string, text = '解释一下'): Message {
  return {
    id,
    text,
    role: 'user',
    thinking: '',
    status: 'done',
    contentRef: null,
    name: null,
  };
}
it('只收录已发送用户消息，保留重复提问的独立身份与顺序', () => {
  expect(
    conversationBookmarks([
      message('a'),
      { ...message('assistant'), role: 'assistant' },
      { ...message('sending'), delivery: { mode: 'steer', state: 'sending' } },
      { ...message('queued'), delivery: { mode: 'followUp', state: 'queued' } },
      message('b'),
    ]),
  ).toEqual([
    { id: 'a', label: '解释一下' },
    { id: 'b', label: '解释一下' },
  ]);
});
it('摘要忽略技能包装，合并空白且不拆开 Unicode', () => {
  expect(
    conversationBookmarks([
      message(
        'a',
        '<skill name="demo" location="C:\\skills\\SKILL.md">\r\n说明\r\n</skill>\r\n\r\n检查\n页面',
      ),
    ])[0].label,
  ).toBe('检查 页面');
  expect(conversationBookmarks([message('a', '😀'.repeat(120))])[0].label).toBe(
    '😀'.repeat(100),
  );
});
it('附件消息有可辨认的摘要，不将文件路径放进目录', () => {
  expect(
    conversationBookmarks([
      { ...message('image', ''), images: [{ id: 'i', mimeType: 'image/png' }] },
    ])[0].label,
  ).toBe('图片 × 1');
  expect(
    conversationBookmarks([
      {
        ...message('file', ''),
        textParts: [
          {
            id: 'f',
            kind: 'files',
            text: '',
            chars: 0,
            contentRef: null,
            truncated: false,
            files: [{ id: 'f', path: 'C:\\secret.txt' }],
          },
        ],
      },
    ])[0].label,
  ).toBe('文件 × 1');
});
