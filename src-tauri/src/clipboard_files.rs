//! 只读取系统文件列表，不读普通文本、不写剪贴板，也不执行剪切操作。
use crate::types::{AppError, Result};
use clipboard_rs::{Clipboard, ClipboardContext, ContentFormat};
use std::path::PathBuf;

/// 平台库处理 Finder URL、CF_HDROP 和 Linux URI 列表；URI 必须转换成本地路径。
fn local_path(value: &str) -> Result<PathBuf> {
    let path = if value.starts_with("file:") {
        let url = url::Url::parse(value)
            .map_err(|_| AppError::new("CLIPBOARD", "剪贴板文件地址无效。"))?;
        if url.host_str().is_some_and(|host| host != "localhost") {
            return Err(AppError::new("CLIPBOARD", "不支持剪贴板中的远程文件地址。"));
        }
        url.to_file_path()
            .map_err(|_| AppError::new("CLIPBOARD", "剪贴板文件地址不是本地路径。"))?
    } else {
        PathBuf::from(value)
    };
    if !path.is_absolute() || value.contains('\0') {
        return Err(AppError::new("CLIPBOARD", "剪贴板文件路径必须是完整路径。"));
    }
    Ok(path)
}

/// 在 blocking 池内创建并释放平台上下文；无文件返回空列表，不伪造路径。
pub fn paths() -> Result<Vec<PathBuf>> {
    let clipboard = ClipboardContext::new()
        .map_err(|_| AppError::new("CLIPBOARD", "无法访问系统剪贴板，请重试或拖入文件。"))?;
    if !clipboard.has(ContentFormat::Files) {
        // GNOME 某些版本只公布专用格式；KDE 的剪切标记不参与文件移动。
        #[cfg(target_os = "linux")]
        {
            let formats = clipboard
                .available_formats()
                .map_err(|_| AppError::new("CLIPBOARD", "无法读取剪贴板格式，请重新复制文件。"))?;
            for format in [
                "x-special/gnome-copied-files",
                "x-special/nautilus-clipboard",
            ] {
                if formats.iter().any(|item| item == format) {
                    let bytes = clipboard
                        .get_buffer(format)
                        .map_err(|_| AppError::new("CLIPBOARD", "无法读取剪贴板文件列表。"))?;
                    return uri_list(&bytes);
                }
            }
        }
        return Ok(vec![]);
    }
    let files = clipboard
        .get_files()
        .map_err(|_| AppError::new("CLIPBOARD", "无法读取剪贴板文件，请重新复制或拖入文件。"))?;
    files.iter().map(|value| local_path(value)).collect()
}

/// Linux 专用列表的 copy/cut 是操作提示，本应用永远只添加引用。
#[cfg(any(target_os = "linux", test))]
fn uri_list(bytes: &[u8]) -> Result<Vec<PathBuf>> {
    let text = std::str::from_utf8(bytes)
        .map_err(|_| AppError::new("CLIPBOARD", "剪贴板文件列表编码无效。"))?;
    text.lines()
        .filter(|line| {
            !line.is_empty() && !line.starts_with('#') && *line != "copy" && *line != "cut"
        })
        .map(local_path)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_local_urls_without_changing_plain_paths() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("中文 空格%23.keys");
        let uri = url::Url::from_file_path(&path).unwrap();
        assert_eq!(local_path(uri.as_str()).unwrap(), path);
        assert_eq!(local_path(path.to_str().unwrap()).unwrap(), path);
    }

    #[test]
    fn parses_copy_and_cut_lists_in_order() {
        let root = tempfile::tempdir().unwrap();
        let paths = [root.path().join("一.keys"), root.path().join("folder")];
        let urls: Vec<_> = paths
            .iter()
            .map(|path| url::Url::from_file_path(path).unwrap())
            .collect();
        for operation in ["copy", "cut"] {
            let text = format!("{operation}\r\n# comment\r\n{}\r\n{}\r\n", urls[0], urls[1]);
            assert_eq!(uri_list(text.as_bytes()).unwrap(), paths);
        }
        assert!(uri_list(&[255]).is_err());
    }

    #[test]
    fn rejects_remote_relative_and_invalid_paths() {
        for value in [
            "file://remote/share/key",
            "prod.keys",
            "https://example.com/key",
            "file:///bad\0key",
        ] {
            assert!(local_path(value).is_err(), "{value:?}");
        }
    }
}
