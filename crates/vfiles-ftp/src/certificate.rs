//! FTPS 自签名证书的生成、持久化与指纹计算。

use std::{
    fs::{self, File, OpenOptions},
    io::{self, Write},
    path::Path,
};

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

    if certificate_path.is_file() && private_key_path.is_file() {
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
    let private_key = generated.signing_key.serialize_pem();

    let mut key_file = create_private_file(private_key_path)?;
    if let Err(err) = key_file
        .write_all(private_key.as_bytes())
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

fn prepare_private_directory(path: &Path) -> io::Result<()> {
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(parent, fs::Permissions::from_mode(0o700))?;
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
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o600))?;
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
