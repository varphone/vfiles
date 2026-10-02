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
    let private_key = private_key_pem(generated.signing_key.serialize_pem());
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

fn private_key_pem(pem: String) -> SecretBox<String> {
    SecretBox::new(Box::new(pem))
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
                    Ok(metadata) => {
                        #[cfg(unix)]
                        {
                            use std::os::unix::fs::PermissionsExt;
                            let mode = metadata.permissions().mode();
                            let writable_by_others = mode & 0o022 != 0;
                            let sticky = mode & 0o1000 != 0;
                            if writable_by_others && !sticky {
                                return Err(io::Error::new(
                                    io::ErrorKind::PermissionDenied,
                                    "FTPS 证书与私钥目录路径不能经过组用户或其他用户可写目录",
                                ));
                            }
                        }
                    }
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

    fn assert_zeroize_on_drop<T: secrecy::zeroize::ZeroizeOnDrop>() {}

    #[test]
    fn private_key_pem_uses_zeroizing_secret_storage() {
        let secret: SecretBox<String> = private_key_pem(String::from("test private key"));

        assert_eq!(secret.expose_secret(), "test private key");
        assert_zeroize_on_drop::<SecretBox<String>>();
    }

    #[test]
    fn generates_persistent_certificate_and_restricts_private_key() {
        let directory = tempfile::tempdir().expect("tempdir");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(directory.path(), fs::Permissions::from_mode(0o700))
                .expect("private test directory");
        }
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

    #[cfg(unix)]
    #[test]
    fn rejects_private_key_symlink_without_changing_its_target() {
        use std::os::unix::{fs::PermissionsExt, fs::symlink};

        let directory = tempfile::tempdir().expect("tempdir");
        fs::set_permissions(directory.path(), fs::Permissions::from_mode(0o700))
            .expect("test directory should be private");
        let certificate_path = directory.path().join("ftp-tls/ftp-cert.pem");
        let private_key_path = directory.path().join("ftp-tls/ftp-key.pem");
        let target_path = directory.path().join("unrelated-file");
        ensure_self_signed_certificate(&certificate_path, &private_key_path, &["localhost".into()])
            .expect("certificate should be generated");

        fs::write(&target_path, b"keep this unrelated file").expect("target should be written");
        fs::set_permissions(&target_path, fs::Permissions::from_mode(0o644))
            .expect("target permissions should be set");
        fs::remove_file(&private_key_path).expect("generated key should be removed");
        symlink(&target_path, &private_key_path).expect("key path should become a symlink");

        let error = ensure_self_signed_certificate(&certificate_path, &private_key_path, &[])
            .expect_err("a symlinked private key must be rejected");

        assert_eq!(error.kind(), io::ErrorKind::InvalidInput);
        assert_eq!(
            fs::read(&target_path).expect("target should remain"),
            b"keep this unrelated file"
        );
        assert_eq!(
            fs::metadata(&target_path)
                .expect("target metadata")
                .permissions()
                .mode()
                & 0o777,
            0o644
        );
    }

    #[cfg(unix)]
    #[test]
    fn rejects_certificate_directory_symlink_without_touching_its_target() {
        use std::os::unix::{fs::PermissionsExt, fs::symlink};

        let directory = tempfile::tempdir().expect("tempdir");
        fs::set_permissions(directory.path(), fs::Permissions::from_mode(0o700))
            .expect("test directory should be private");
        let target_directory = directory.path().join("unrelated-directory");
        fs::create_dir(&target_directory).expect("target directory should be created");
        fs::set_permissions(&target_directory, fs::Permissions::from_mode(0o755))
            .expect("target directory permissions should be set");
        let certificate_directory = directory.path().join("ftp-tls");
        symlink(&target_directory, &certificate_directory)
            .expect("certificate directory should become a symlink");

        let error = ensure_self_signed_certificate(
            &certificate_directory.join("ftp-cert.pem"),
            &certificate_directory.join("ftp-key.pem"),
            &["localhost".into()],
        )
        .expect_err("a symlinked certificate directory must be rejected");

        assert_eq!(error.kind(), io::ErrorKind::InvalidInput);
        assert_eq!(
            fs::metadata(&target_directory)
                .expect("target directory metadata")
                .permissions()
                .mode()
                & 0o777,
            0o755
        );
        assert_eq!(
            fs::read_dir(&target_directory)
                .expect("target directory should remain readable")
                .count(),
            0
        );
    }

    #[cfg(unix)]
    #[test]
    fn rejects_group_writable_ancestor_before_creating_certificate_files() {
        use std::os::unix::fs::PermissionsExt;

        let directory = tempfile::tempdir().expect("tempdir");
        fs::set_permissions(directory.path(), fs::Permissions::from_mode(0o700))
            .expect("test directory should be private");
        let shared_directory = directory.path().join("shared");
        fs::create_dir(&shared_directory).expect("shared directory should be created");
        fs::set_permissions(&shared_directory, fs::Permissions::from_mode(0o770))
            .expect("shared directory permissions should be set");
        let certificate_directory = shared_directory.join("nested/ftp-tls");

        let error = ensure_self_signed_certificate(
            &certificate_directory.join("ftp-cert.pem"),
            &certificate_directory.join("ftp-key.pem"),
            &["localhost".into()],
        )
        .expect_err("a group-writable ancestor must be rejected");

        assert_eq!(error.kind(), io::ErrorKind::PermissionDenied);
        assert!(!certificate_directory.exists());
        assert_eq!(
            fs::metadata(&shared_directory)
                .expect("shared directory metadata")
                .permissions()
                .mode()
                & 0o777,
            0o770
        );
    }
}
