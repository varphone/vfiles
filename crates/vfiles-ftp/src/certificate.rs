//! FTPS 自签名证书的生成、持久化与指纹计算。

use std::{
    fs::{self, File, OpenOptions},
    io::{self, Write},
    path::{Component, Path, PathBuf},
};

use secrecy::{ExposeSecret, SecretBox};
use sha2::{Digest, Sha256};

/// 确保证书与私钥存在；仅首次启动生成，后续启动复用同一身份。
///
/// 返回证书 PEM 文件的 SHA-256，便于客户端通过可信渠道核对证书。
pub fn ensure_self_signed_certificate(
    certificate_path: &Path,
    private_key_path: &Path,
    subject_alt_names: &[String],
) -> io::Result<String> {
    if certificate_path == private_key_path {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "FTPS 证书和私钥不能使用同一路径",
        ));
    }

    prepare_private_directory(certificate_path)?;
    prepare_private_directory(private_key_path)?;

    let certificate_exists = regular_file_exists(certificate_path)?;
    let private_key_exists = regular_file_exists(private_key_path)?;
    if certificate_exists && private_key_exists {
        restrict_private_file(private_key_path)?;
        return fingerprint(certificate_path);
    }

    remove_if_exists(certificate_path)?;
    remove_if_exists(private_key_path)?;

    let names = if subject_alt_names.is_empty() {
        vec!["localhost".to_string()]
    } else {
        subject_alt_names.to_vec()
    };
    let generated = rcgen::generate_simple_self_signed(names)
        .map_err(|err| io::Error::other(format!("生成 FTPS 自签名证书失败: {err}")))?;
    let certificate = generated.cert.pem();
    let private_key = SecretBox::new(Box::new(generated.signing_key.serialize_pem()));
    drop(generated);

    let mut key_file = create_private_file(private_key_path)?;
    if let Err(err) = key_file
        .write_all(private_key.expose_secret().as_bytes())
        .and_then(|()| key_file.sync_all())
    {
        remove_if_exists(private_key_path)?;
        return Err(err);
    }

    let mut cert_file = match create_certificate_file(certificate_path) {
        Ok(file) => file,
        Err(err) => {
            remove_if_exists(private_key_path)?;
            return Err(err);
        }
    };
    if let Err(err) = cert_file
        .write_all(certificate.as_bytes())
        .and_then(|()| cert_file.sync_all())
    {
        remove_if_exists(certificate_path)?;
        remove_if_exists(private_key_path)?;
        return Err(err);
    }

    fingerprint(certificate_path)
}

fn fingerprint(certificate_path: &Path) -> io::Result<String> {
    let certificate = fs::read(certificate_path)?;
    Ok(hex::encode(Sha256::digest(certificate)))
}

fn remove_if_exists(path: &Path) -> io::Result<()> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(err) if err.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(err) => Err(err),
    }
}

fn regular_file_exists(path: &Path) -> io::Result<bool> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_file() => Ok(true),
        Ok(_) => Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "FTPS 证书与私钥路径必须指向普通文件，不能是符号链接",
        )),
        Err(err) if err.kind() == io::ErrorKind::NotFound => Ok(false),
        Err(err) => Err(err),
    }
}

fn prepare_private_directory(path: &Path) -> io::Result<()> {
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    reject_symlinked_parents(parent)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        let mut builder = fs::DirBuilder::new();
        builder.recursive(true).mode(0o700).create(parent)?;
    }
    #[cfg(not(unix))]
    fs::create_dir_all(parent)?;
    reject_symlinked_parents(parent)?;
    let metadata = fs::symlink_metadata(parent)?;
    if !metadata.file_type().is_dir() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "FTPS 证书与私钥目录不能是符号链接，且必须是目录",
        ));
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if metadata.permissions().mode() & 0o022 != 0 {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "FTPS 证书与私钥目录不能允许组用户或其他用户写入",
            ));
        }
    }
    Ok(())
}

fn reject_symlinked_parents(parent: &Path) -> io::Result<()> {
    let mut current = if parent.is_absolute() {
        PathBuf::new()
    } else {
        std::env::current_dir()?
    };

    for component in parent.components() {
        match component {
            Component::Prefix(prefix) => current.push(prefix.as_os_str()),
            Component::RootDir => current.push(component.as_os_str()),
            Component::CurDir => {}
            Component::ParentDir => {
                current.pop();
            }
            Component::Normal(name) => {
                current.push(name);
                match fs::symlink_metadata(&current) {
                    Ok(metadata) if metadata.file_type().is_symlink() => {
                        return Err(io::Error::new(
                            io::ErrorKind::InvalidInput,
                            "FTPS 证书与私钥目录路径不能经过符号链接",
                        ));
                    }
                    Ok(metadata) if !metadata.file_type().is_dir() => {
                        return Err(io::Error::new(
                            io::ErrorKind::InvalidInput,
                            "FTPS 证书与私钥目录路径必须由目录组成",
                        ));
                    }
                    Ok(_) => {}
                    Err(err) if err.kind() == io::ErrorKind::NotFound => {}
                    Err(err) => return Err(err),
                }
            }
        }
    }
    Ok(())
}

fn create_private_file(path: &Path) -> io::Result<File> {
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    options.open(path)
}

fn create_certificate_file(path: &Path) -> io::Result<File> {
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o644);
    }
    options.open(path)
}

fn restrict_private_file(path: &Path) -> io::Result<()> {
    let metadata = fs::symlink_metadata(path)?;
    if !metadata.file_type().is_file() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "FTPS 私钥路径必须指向普通文件，不能是符号链接",
        ));
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let file = OpenOptions::new().read(true).open(path)?;
        file.set_permissions(fs::Permissions::from_mode(0o600))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generates_persistent_certificate_and_restricts_private_key() {
        let directory = tempfile::tempdir().expect("tempdir");
        let certificate_path = directory.path().join("ftp-tls/ftp-cert.pem");
        let private_key_path = directory.path().join("ftp-tls/ftp-key.pem");
        let names = vec!["localhost".to_string(), "127.0.0.1".to_string()];

        let first_fingerprint =
            ensure_self_signed_certificate(&certificate_path, &private_key_path, &names)
                .expect("certificate should be generated");
        let first_certificate = fs::read(&certificate_path).expect("certificate should exist");
        let first_key = fs::read(&private_key_path).expect("key should exist");

        let second_fingerprint = ensure_self_signed_certificate(
            &certificate_path,
            &private_key_path,
            &["changed.example".to_string()],
        )
        .expect("certificate should be reused");

        assert_eq!(first_fingerprint, second_fingerprint);
        assert_eq!(
            fs::read(&certificate_path).expect("certificate"),
            first_certificate
        );
        assert_eq!(fs::read(&private_key_path).expect("key"), first_key);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                fs::metadata(&private_key_path)
                    .expect("key metadata")
                    .permissions()
                    .mode()
                    & 0o777,
                0o600
            );
            assert_eq!(
                fs::metadata(certificate_path.parent().expect("certificate directory"))
                    .expect("directory metadata")
                    .permissions()
                    .mode()
                    & 0o777,
                0o700
            );
        }
    }
}
