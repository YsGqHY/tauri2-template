//! AES-256-GCM helpers for internal Rust callers.
//!
//! The key is stored as 32 random bytes in `.master.key` beneath the supplied
//! user-data directory. The encoded payload is base64 of:
//! `NFCR` (magic) + version byte + 12-byte nonce + AES-GCM ciphertext/tag.
//!
//! This module never logs key material, plaintext, or ciphertext. The local
//! key file protects against casual database copying, not a local administrator;
//! Windows permissions are best-effort because this module does not manage ACLs.

use aes_gcm::aead::{rand_core::RngCore, Aead, KeyInit, OsRng};
use aes_gcm::{Aes256Gcm, Nonce};
use base64::Engine;
use std::fmt;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Write};
use std::path::{Component, Path, PathBuf};

/// The key file name used beneath an application-owned user-data directory.
pub const MASTER_KEY_FILE_NAME: &str = ".master.key";
const KEY_SIZE: usize = 32;
const NONCE_SIZE: usize = 12;
const TAG_SIZE: usize = 16;
const VERSION: u8 = 1;
const MAGIC: &[u8; 4] = b"NFCR";

/// Errors intentionally avoid including secrets, paths, URLs, or ciphertext.
#[derive(Debug)]
pub enum CryptoError {
    Io(io::Error),
    InvalidInput,
    InvalidKey,
    InvalidCiphertext,
    UnsupportedVersion,
    CipherOperation,
}

impl fmt::Display for CryptoError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let message = match self {
            Self::Io(_) => "cryptox I/O failure",
            Self::InvalidInput => "cryptox invalid input",
            Self::InvalidKey => "cryptox invalid master key",
            Self::InvalidCiphertext => "cryptox invalid ciphertext",
            Self::UnsupportedVersion => "cryptox unsupported ciphertext version",
            Self::CipherOperation => "cryptox cipher operation failed",
        };
        formatter.write_str(message)
    }
}

impl std::error::Error for CryptoError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(error) => Some(error),
            _ => None,
        }
    }
}

impl From<io::Error> for CryptoError {
    fn from(error: io::Error) -> Self {
        Self::Io(error)
    }
}

/// Builds the path to `.master.key` directly beneath `user_data_dir`.
pub fn master_key_path(user_data_dir: impl AsRef<Path>) -> io::Result<PathBuf> {
    let user_data_dir = user_data_dir.as_ref();
    if user_data_dir.as_os_str().is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "user data directory must not be empty",
        ));
    }
    Ok(user_data_dir.join(MASTER_KEY_FILE_NAME))
}

/// Encrypts a string with the per-user-data-directory AES-256-GCM key.
pub fn encrypt_string(
    user_data_dir: impl AsRef<Path>,
    plaintext: impl AsRef<[u8]>,
) -> Result<String, CryptoError> {
    let key = load_or_create_key(user_data_dir)?;
    let cipher = Aes256Gcm::new_from_slice(&key).map_err(|_| CryptoError::InvalidKey)?;
    let mut nonce_bytes = [0_u8; NONCE_SIZE];
    OsRng.fill_bytes(&mut nonce_bytes);
    let nonce = Nonce::from_slice(&nonce_bytes);
    let ciphertext = cipher
        .encrypt(nonce, plaintext.as_ref())
        .map_err(|_| CryptoError::CipherOperation)?;

    let mut payload = Vec::with_capacity(MAGIC.len() + 1 + NONCE_SIZE + ciphertext.len());
    payload.extend_from_slice(MAGIC);
    payload.push(VERSION);
    payload.extend_from_slice(&nonce_bytes);
    payload.extend_from_slice(&ciphertext);
    Ok(base64::engine::general_purpose::STANDARD.encode(payload))
}

/// Decrypts a payload produced by [`encrypt_string`].
pub fn decrypt_string(
    user_data_dir: impl AsRef<Path>,
    encoded: &str,
) -> Result<Vec<u8>, CryptoError> {
    if encoded.is_empty() {
        return Ok(Vec::new());
    }
    let raw = base64::engine::general_purpose::STANDARD
        .decode(encoded)
        .map_err(|_| CryptoError::InvalidCiphertext)?;
    let header_len = MAGIC.len() + 1 + NONCE_SIZE;
    if raw.len() < header_len + TAG_SIZE || &raw[..MAGIC.len()] != MAGIC {
        return Err(CryptoError::InvalidCiphertext);
    }
    if raw[MAGIC.len()] != VERSION {
        return Err(CryptoError::UnsupportedVersion);
    }

    let key = load_or_create_key(user_data_dir)?;
    let cipher = Aes256Gcm::new_from_slice(&key).map_err(|_| CryptoError::InvalidKey)?;
    let nonce_start = MAGIC.len() + 1;
    let nonce_end = nonce_start + NONCE_SIZE;
    let nonce = Nonce::from_slice(&raw[nonce_start..nonce_end]);
    cipher
        .decrypt(nonce, &raw[nonce_end..])
        .map_err(|_| CryptoError::InvalidCiphertext)
}

/// Best-effort owner-only permissions for an existing data directory on Unix.
/// Windows intentionally performs no ACL changes. The directory is validated
/// and is never created by this helper.
pub fn set_private_data_dir_permissions(path: impl AsRef<Path>) -> io::Result<()> {
    let path = path.as_ref();
    #[cfg(unix)]
    {
        let metadata = fs::symlink_metadata(path)?;
        if metadata.file_type().is_symlink() || !metadata.is_dir() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "data directory must be a real directory, not a symlink",
            ));
        }
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o700))
    }
    #[cfg(not(unix))]
    {
        let _ = path;
        Ok(())
    }
}

/// Best-effort owner-only permissions for an existing `.master.key` on Unix.
/// The path must be a regular non-symlink file. Windows intentionally performs
/// no ACL changes.
pub fn set_master_key_permissions(user_data_dir: impl AsRef<Path>) -> io::Result<()> {
    let key_path = master_key_path(user_data_dir)?;
    #[cfg(unix)]
    {
        let metadata = fs::symlink_metadata(&key_path)?;
        if metadata.file_type().is_symlink() || !metadata.is_file() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "master key must be a regular file, not a symlink",
            ));
        }
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&key_path, fs::Permissions::from_mode(0o600))
    }
    #[cfg(not(unix))]
    {
        let _ = key_path;
        Ok(())
    }
}

fn load_or_create_key(user_data_dir: impl AsRef<Path>) -> Result<[u8; KEY_SIZE], CryptoError> {
    let user_data_dir = user_data_dir.as_ref();
    ensure_user_data_dir(user_data_dir)?;
    let key_path = master_key_path(user_data_dir)?;

    if let Ok(metadata) = fs::symlink_metadata(&key_path) {
        if metadata.file_type().is_symlink() || !metadata.is_file() {
            return Err(CryptoError::InvalidKey);
        }
        let key = read_key(&key_path)?;
        set_master_key_permissions(user_data_dir)?;
        return Ok(key);
    }

    let mut key = [0_u8; KEY_SIZE];
    OsRng.fill_bytes(&mut key);
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }

    match options.open(&key_path) {
        Ok(mut file) => {
            let result = (|| -> Result<(), CryptoError> {
                file.write_all(&key)?;
                file.sync_all()?;
                drop(file);
                set_master_key_permissions(user_data_dir)?;
                Ok(())
            })();
            if result.is_err() {
                let _ = fs::remove_file(&key_path);
            }
            result?;
            Ok(key)
        }
        Err(error) if error.kind() == io::ErrorKind::AlreadyExists => read_key(&key_path),
        Err(error) => Err(CryptoError::Io(error)),
    }
}

fn read_key(path: &Path) -> Result<[u8; KEY_SIZE], CryptoError> {
    let metadata = fs::symlink_metadata(path).map_err(|_| CryptoError::InvalidKey)?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err(CryptoError::InvalidKey);
    }
    let mut file = File::open(path).map_err(|_| CryptoError::InvalidKey)?;
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes)
        .map_err(|_| CryptoError::InvalidKey)?;
    if bytes.len() != KEY_SIZE {
        return Err(CryptoError::InvalidKey);
    }
    let mut key = [0_u8; KEY_SIZE];
    key.copy_from_slice(&bytes);
    Ok(key)
}

fn ensure_user_data_dir(path: &Path) -> Result<(), CryptoError> {
    if path.as_os_str().is_empty() {
        return Err(CryptoError::InvalidInput);
    }
    reject_symlink_components(path)?;
    fs::create_dir_all(path)?;
    let metadata = fs::symlink_metadata(path)?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err(CryptoError::InvalidInput);
    }
    set_private_data_dir_permissions(path)?;
    Ok(())
}

fn reject_symlink_components(path: &Path) -> Result<(), CryptoError> {
    let mut current = PathBuf::new();
    for component in path.components() {
        match component {
            Component::Prefix(_) | Component::RootDir => current.push(component.as_os_str()),
            Component::CurDir => {}
            Component::ParentDir => current.push(component.as_os_str()),
            Component::Normal(_) => current.push(component.as_os_str()),
        }
        if let Ok(metadata) = fs::symlink_metadata(&current) {
            if metadata.file_type().is_symlink() {
                return Err(CryptoError::InvalidInput);
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use base64::Engine;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn roundtrip_and_empty_payload() {
        let directory = tempdir().expect("create temp directory");
        let encrypted = encrypt_string(directory.path(), "secret text").expect("encrypt");
        assert_eq!(
            decrypt_string(directory.path(), &encrypted).expect("decrypt"),
            b"secret text"
        );
        assert_eq!(
            decrypt_string(directory.path(), "").expect("empty decrypt"),
            b""
        );
    }

    #[test]
    fn tampering_is_rejected_without_plaintext_error_details() {
        let directory = tempdir().expect("create temp directory");
        let encrypted = encrypt_string(directory.path(), "secret text").expect("encrypt");
        let mut raw = base64::engine::general_purpose::STANDARD
            .decode(encrypted)
            .expect("decode ciphertext");
        *raw.last_mut().expect("ciphertext byte") ^= 1;
        let tampered = base64::engine::general_purpose::STANDARD.encode(raw);
        let error = decrypt_string(directory.path(), &tampered).expect_err("reject tampering");
        assert!(matches!(error, CryptoError::InvalidCiphertext));
        assert!(!error.to_string().contains("secret text"));
    }

    #[test]
    fn separate_instances_read_the_same_key() {
        let directory = tempdir().expect("create temp directory");
        let first = encrypt_string(directory.path(), "from first").expect("encrypt first");
        let second = encrypt_string(directory.path(), "from second").expect("encrypt second");
        assert_ne!(first, second);
        assert_eq!(
            decrypt_string(directory.path(), &first).expect("decrypt first"),
            b"from first"
        );
        assert_eq!(
            decrypt_string(directory.path(), &second).expect("decrypt second"),
            b"from second"
        );
    }

    #[test]
    fn wrong_key_and_invalid_key_length_are_rejected() {
        let first_dir = tempdir().expect("create first directory");
        let second_dir = tempdir().expect("create second directory");
        let encrypted = encrypt_string(first_dir.path(), "secret").expect("encrypt");
        let error = decrypt_string(second_dir.path(), &encrypted).expect_err("reject wrong key");
        assert!(matches!(error, CryptoError::InvalidCiphertext));

        let invalid_dir = tempdir().expect("create invalid-key directory");
        fs::write(
            master_key_path(invalid_dir.path()).expect("key path"),
            [0_u8; 31],
        )
        .expect("write invalid key");
        let error = encrypt_string(invalid_dir.path(), "secret").expect_err("reject invalid key");
        assert!(matches!(error, CryptoError::InvalidKey));
    }

    #[test]
    fn header_and_unknown_version_are_rejected() {
        let directory = tempdir().expect("create temp directory");
        let encrypted = encrypt_string(directory.path(), "secret").expect("encrypt");
        let mut raw = base64::engine::general_purpose::STANDARD
            .decode(encrypted)
            .expect("decode ciphertext");
        raw[0] ^= 1;
        let error = decrypt_string(
            directory.path(),
            &base64::engine::general_purpose::STANDARD.encode(raw),
        )
        .expect_err("reject bad magic");
        assert!(matches!(error, CryptoError::InvalidCiphertext));

        let encrypted = encrypt_string(directory.path(), "secret").expect("encrypt again");
        let mut raw = base64::engine::general_purpose::STANDARD
            .decode(encrypted)
            .expect("decode ciphertext");
        raw[MAGIC.len()] = VERSION + 1;
        let error = decrypt_string(
            directory.path(),
            &base64::engine::general_purpose::STANDARD.encode(raw),
        )
        .expect_err("reject unknown version");
        assert!(matches!(error, CryptoError::UnsupportedVersion));
    }

    #[cfg(unix)]
    #[test]
    fn key_permissions_and_symlink_boundary_are_enforced() {
        use std::os::unix::fs::{symlink, PermissionsExt};

        let root = tempdir().expect("create root directory");
        let data_dir = root.path().join("data");
        encrypt_string(&data_dir, "secret").expect("create key");
        assert_eq!(
            fs::metadata(&data_dir)
                .expect("stat data directory")
                .permissions()
                .mode()
                & 0o777,
            0o700
        );
        let key = master_key_path(&data_dir).expect("key path");
        assert_eq!(
            fs::metadata(key).expect("stat key").permissions().mode() & 0o777,
            0o600
        );

        let link = root.path().join("data-link");
        symlink(&data_dir, &link).expect("create data directory symlink");
        assert!(matches!(
            encrypt_string(&link, "secret"),
            Err(CryptoError::InvalidInput)
        ));
    }
}
