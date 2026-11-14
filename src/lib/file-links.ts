/** 文件引用只描述用户点击的目标；实际读取与打开仍由宿主验证。 */
export interface FileTarget {
  path: string;
  line?: number;
}
export const fileLinkKey = Symbol('file-link');
export type OpenFile = (target: FileTarget, trigger: HTMLElement) => void;

/** 裸域名采用保守后缀集，避免把 main.ts、README.md 等文件当成网站；其他网站可显式写 https://。 */
const websiteSuffixes = new Set(
  'com org net edu gov mil int io ai app dev co me info biz xyz online site tech store cloud cc si cn uk de fr jp us ca au in tv gg ly'.split(
    ' ',
  ),
);

/** 只为明确的裸域名补全 HTTPS；显式相对路径、盘符、共享路径及认证信息不参与推断。 */
export function websiteTarget(value: string): string | null {
  if (/[\s\\\u0000-\u001f\u007f]/.test(value)) return null;
  const match = value.match(
    /^((?:[a-z\d](?:[a-z\d-]{0,61}[a-z\d])?\.)+([a-z]{2,}))(?::(\d{1,5}))?(?:[/?#][^<>"`]*|$)$/i,
  );
  if (!match || !websiteSuffixes.has(match[2].toLowerCase())) return null;
  try {
    return new URL(`https://${value}`).href;
  } catch {
    return null;
  }
}

/** Markdown URL 解码一次；行内代码是原始文件名，不能误解码其中的百分号。 */
export function fileTarget(value: string, encoded = true): FileTarget | null {
  let path = value.trim();
  if (!path || /[\u0000-\u001f\u007f]/.test(path) || websiteTarget(path))
    return null;
  const location = path.match(
    /(?:#L([1-9]\d*)(?:-L?\d+)?|:([1-9]\d*)(?::\d+)?)$/,
  );
  const line = location ? Number(location[1] ?? location[2]) : undefined;
  if (location) path = path.slice(0, -location[0].length);
  if (/^file:/i.test(path)) {
    try {
      const url = new URL(path);
      if (url.hostname && url.hostname !== 'localhost') return null;
      path = url.pathname;
      if (/^\/[a-z]:\//i.test(path)) path = path.slice(1);
    } catch {
      return null;
    }
  } else if (/^[a-z][a-z\d+.-]*:/i.test(path) && !/^[a-z]:[\\/]/i.test(path))
    return null;
  if (encoded) {
    try {
      path = decodeURIComponent(path);
    } catch {
      return null;
    }
  }
  if (
    !path ||
    /[\u0000-\u001f\u007f]/.test(path) ||
    path.startsWith('//') ||
    path.startsWith('\\\\')
  )
    return null;
  return { path, ...(line && Number.isSafeInteger(line) ? { line } : {}) };
}

/** 保守识别完整的行内代码路径，不扫描代码块、表达式、命令或一般正文。 */
export function codeFileTarget(value: string): FileTarget | null {
  if (/\s|[<>|*?`"'(){}=;]/.test(value)) return null;
  const target = fileTarget(value, false);
  if (!target || !/(?:^|[\\/])(?:\.[\w-]+|[^\\/]+\.[\w-]+)$/.test(target.path))
    return null;
  return target;
}

export interface FilePreview {
  path: string;
  kind: 'text' | 'image' | 'other' | 'directory' | 'approval';
  text?: string;
  image?: string;
  truncated?: boolean;
  size: number;
  canOpen: boolean;
}
