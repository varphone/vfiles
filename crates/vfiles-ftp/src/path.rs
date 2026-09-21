//! FTP 路径处理：把客户端路径映射为命名空间内的规范化路径。
//!
//! FTP 客户端发送的路径可能是 `/a/b`、`a/b`、`.`、`..`；这里统一为
//! `NormalizedPath`（无前导 `/`、无空段）。`..` 采用**向上夹取**语义：
//! 在命名空间根之上继续向上仍停留在根，因此客户端可以正常 `CDUP`/`CWD ..`，
//! 但永远无法越出该用户的命名空间。

use std::path::{Component, Path};

use unftp_core::storage::{Error, ErrorKind};
use vfiles_domain::NormalizedPath;

/// 把 FTP 路径转换为命名空间内的相对路径。
pub fn to_normalized(path: &Path) -> Result<NormalizedPath, Error> {
    let raw = path
        .to_str()
        .ok_or_else(|| Error::new(ErrorKind::PermissionDenied, "文件名不是合法的 UTF-8 字符串"))?;

    if raw.contains('\0') {
        return Err(Error::new(ErrorKind::PermissionDenied, "路径包含非法字符"));
    }

    let mut segments: Vec<&str> = Vec::new();
    for component in Path::new(raw).components() {
        match component {
            Component::RootDir | Component::CurDir | Component::Prefix(_) => {}
            Component::ParentDir => {
                // 夹取在根：`/../x` 视为 `/x`，`CWD ..` 在根目录保持不动
                segments.pop();
            }
            Component::Normal(part) => {
                let part = part.to_str().ok_or_else(|| {
                    Error::new(ErrorKind::PermissionDenied, "文件名不是合法的 UTF-8 字符串")
                })?;
                if part.contains('\\') {
                    return Err(Error::new(
                        ErrorKind::PermissionDenied,
                        "路径不允许包含反斜杠",
                    ));
                }
                segments.push(part);
            }
        }
    }

    let joined = segments.join("/");
    NormalizedPath::new(&joined)
        .map_err(|message| Error::new(ErrorKind::PermissionDenied, format!("非法路径: {message}")))
}

/// 把命名空间内路径渲染成 FTP 客户端看到的绝对路径（列表用）。
pub fn to_client_path(path: &NormalizedPath) -> String {
    if path.as_str().is_empty() {
        "/".to_string()
    } else {
        format!("/{}", path.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn normalize(value: &str) -> Result<NormalizedPath, Error> {
        to_normalized(&PathBuf::from(value))
    }

    #[test]
    fn maps_absolute_and_relative_paths_to_namespace_paths() {
        assert_eq!(normalize("/").expect("root").as_str(), "");
        assert_eq!(normalize(".").expect("dot").as_str(), "");
        assert_eq!(normalize("/a/b.txt").expect("absolute").as_str(), "a/b.txt");
        assert_eq!(normalize("a/b.txt").expect("relative").as_str(), "a/b.txt");
        assert_eq!(normalize("a//b").expect("double slash").as_str(), "a/b");
        assert_eq!(normalize("/dir/").expect("trailing").as_str(), "dir");
    }

    #[test]
    fn clamps_parent_segments_at_the_namespace_root() {
        // 客户端常用的 CDUP / CWD .. 必须可用，但不能越出命名空间
        assert_eq!(normalize("..").expect("parent").as_str(), "");
        assert_eq!(
            normalize("/../etc/passwd").expect("root escape").as_str(),
            "etc/passwd"
        );
        assert_eq!(normalize("a/b/..").expect("nested parent").as_str(), "a");
        assert_eq!(normalize("a/../../b").expect("over-pop").as_str(), "b");
    }

    #[test]
    fn rejects_invalid_characters() {
        assert!(normalize("a\\b").is_err());
        assert!(normalize("a\0b").is_err());
    }

    #[test]
    fn renders_client_paths_with_leading_slash() {
        assert_eq!(to_client_path(&NormalizedPath::new("").expect("root")), "/");
        assert_eq!(
            to_client_path(&NormalizedPath::new("a/b").expect("path")),
            "/a/b"
        );
    }
}
