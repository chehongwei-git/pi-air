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
  // AI 常用 /C:/… 表示 Windows 绝对链接；解码后统一处理，避免宿主将盘符拼进项目目录。
  if (/^\/[a-z]:[\\/]/i.test(path)) path = path.slice(1);
  return { path, ...(line && Number.isSafeInteger(line) ? { line } : {}) };
}

/** 自动链接采用常见后缀与约定文件名，宁可遗漏特殊文件，也不把任意对象属性当成路径。 */
const fileSuffixes = new Set(
  'txt md markdown mdown log csv tsv json jsonc jsonl yaml yml toml xml ini conf config rs ts tsx js jsx mjs cjs mts cts java css scss sass less html htm svelte vue c h cc cpp hpp go py pyi rb php sh bash zsh fish ps1 bat cmd sql kt kts swift m mm cs fs fsx dart lua r ipynb proto graphql gql prisma lock pdf doc docx xls xlsx ppt pptx odt ods odp rtf png jpg jpeg gif webp bmp svg ico tiff tif heic avif mp3 wav ogg flac mp4 mov webm mkv zip gz tar tgz 7z rar wasm'.split(
    ' ',
  ),
);
const fileNames = new Set(
  '.gitignore .gitattributes .gitmodules .dockerignore .editorconfig .npmrc .nvmrc .prettierrc .eslintrc .env dockerfile containerfile makefile gnumakefile license licence readme changelog'.split(
    ' ',
  ),
);

/** 保守识别完整的行内代码路径，不扫描代码块、表达式、命令或一般正文。 */
export function codeFileTarget(value: string): FileTarget | null {
  if (/\s|[<>|*?`"'(){}=;]/.test(value)) return null;
  const target = fileTarget(value, false);
  if (!target) return null;
  const name = target.path.split(/[\\/]/).pop() ?? '';
  if (!name || name === '.' || name === '..') return null;
  // 明确路径允许未知后缀与无扩展名文件；此处仅推断意图，不访问磁盘确认存在性。
  if (/[\\/]/.test(target.path)) return target;
  const lower = name.toLowerCase();
  const dot = lower.lastIndexOf('.');
  return fileNames.has(lower) ||
    /^\.env\..+/.test(lower) ||
    (dot > 0 && fileSuffixes.has(lower.slice(dot + 1)))
    ? target
    : null;
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
