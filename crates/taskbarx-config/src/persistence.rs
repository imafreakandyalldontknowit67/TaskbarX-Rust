use std::env;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};

use crate::settings::MAX_JSON_BYTES;
use crate::{from_json, to_json, Settings};

/// Resolve `%APPDATA%/TaskbarX-Rust/settings.json` without filesystem writes.
pub fn default_path() -> Result<PathBuf, String> {
    let appdata = env::var_os("APPDATA").filter(|value| !value.is_empty()).ok_or_else(|| {
        "APPDATA is unavailable or empty; set APPDATA to your roaming application-data directory or supply an explicit settings path".to_owned()
    })?;
    let appdata = PathBuf::from(appdata);
    if !appdata.is_absolute() {
        return Err("APPDATA must be an absolute directory path; correct APPDATA or supply an explicit settings path".into());
    }
    Ok(appdata.join("TaskbarX-Rust").join("settings.json"))
}

/// Read and validate settings. Only a missing file yields defaults; no writes
/// occur, including when parent directories are missing or the file is corrupt.
pub fn load(path: &Path) -> Result<Settings, String> {
    let file = match File::open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            // A dangling symlink is an existing, broken configuration, not a
            // fresh installation. Do not quietly hide it with defaults.
            return match fs::symlink_metadata(path) {
                Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(Settings::default()),
                _ => Err(format!(
                    "could not open settings {}: {error}",
                    path.display()
                )),
            };
        }
        Err(error) => {
            return Err(format!(
                "could not open settings {}: {error}",
                path.display()
            ))
        }
    };
    // Limit the read itself, rather than trusting metadata that can change while
    // another process is writing. The extra byte distinguishes exactly 64 KiB.
    let mut bytes = Vec::new();
    file.take((MAX_JSON_BYTES + 1) as u64)
        .read_to_end(&mut bytes)
        .map_err(|error| format!("could not read settings {}: {error}", path.display()))?;
    if bytes.len() > MAX_JSON_BYTES {
        return Err(format!(
            "settings {} exceeds the 64 KiB (65536 byte) limit",
            path.display()
        ));
    }
    let text = std::str::from_utf8(&bytes)
        .map_err(|error| format!("settings {} is not UTF-8: {error}", path.display()))?;
    from_json(text).map_err(|error| format!("settings {}: {error}", path.display()))
}

/// Validate, create parent directories if necessary, and atomically save.
///
/// A random, exclusively created temporary file in the destination directory is
/// fully written, flushed, and synced before replacement. Windows uses a
/// same-volume write-through rename with replacement, never delete-then-rename
/// or a cross-volume copy fallback. Concurrent writers use independent files;
/// the last successful replacement wins. No external lock is needed. Windows
/// sharing/access conflicts are retried briefly, then returned to the caller.
///
/// On validation, write, or replacement failure, the previous destination is
/// untouched. Cleanup is limited to this operation's temporary file. A killed
/// process can leave its temporary file behind; later saves do not remove it.
/// Durability ultimately depends on the filesystem and storage hardware. On
/// Unix, a directory-sync error after replacement is reported explicitly: the
/// new settings are already installed and must not be rolled back over a
/// potentially newer concurrent save.
pub fn save_atomic(path: &Path, settings: &Settings) -> Result<(), String> {
    // Validate before any filesystem changes, including directory creation.
    let text = to_json(settings)?;
    atomic_write(path, |file| file.write_all(text.as_bytes()))
}

fn atomic_write(
    path: &Path,
    write: impl FnOnce(&mut File) -> io::Result<()>,
) -> Result<(), String> {
    let filename = path
        .file_name()
        .ok_or_else(|| format!("settings path {} must name a file", path.display()))?;
    let parent = path
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    fs::create_dir_all(parent).map_err(|error| {
        format!(
            "could not create settings directory {}: {error}",
            parent.display()
        )
    })?;
    // Canonicalizing the parent keeps both names in the same directory, handles
    // relative paths, and gives Windows extended-length Unicode paths.
    let parent = fs::canonicalize(parent).map_err(|error| {
        format!(
            "could not resolve settings directory {}: {error}",
            parent.display()
        )
    })?;
    let destination = parent.join(filename);
    #[cfg(unix)]
    let directory = File::open(&parent).map_err(|error| {
        format!(
            "could not open settings directory {}: {error}",
            parent.display()
        )
    })?;

    let mut temporary = TemporaryFile::create(&parent).map_err(|error| {
        format!(
            "could not create temporary settings file in {}: {error}",
            parent.display()
        )
    })?;
    let file = temporary.file.as_mut().expect("temporary file is open");
    write(file)
        .and_then(|()| file.flush())
        .and_then(|()| file.sync_all())
        .map_err(|error| {
            format!(
                "could not write and flush settings for {}: {error}",
                path.display()
            )
        })?;
    // Close before renaming, especially for Windows handles opened without
    // sharing. Drop also closes before trying cleanup on any earlier failure.
    drop(temporary.file.take());
    replace(&temporary.path, &destination).map_err(|error| {
        format!(
            "could not atomically replace settings {}: {error}",
            path.display()
        )
    })?;
    temporary.committed = true;

    #[cfg(unix)]
    directory.sync_all().map_err(|error| {
        format!(
            "settings {} was replaced, but syncing its directory failed: {error}",
            path.display()
        )
    })?;
    Ok(())
}

struct TemporaryFile {
    path: PathBuf,
    file: Option<File>,
    committed: bool,
}

impl TemporaryFile {
    fn create(parent: &Path) -> io::Result<Self> {
        for _ in 0..32 {
            let path = parent.join(format!(".taskbarx-settings-{:032x}.tmp", random_nonce()?));
            let mut options = OpenOptions::new();
            options.write(true).create_new(true);
            #[cfg(windows)]
            {
                use std::os::windows::fs::OpenOptionsExt;
                options.share_mode(0);
            }
            #[cfg(unix)]
            {
                use std::os::unix::fs::OpenOptionsExt;
                options.mode(0o600);
            }
            match options.open(&path) {
                Ok(file) => {
                    return Ok(Self {
                        path,
                        file: Some(file),
                        committed: false,
                    })
                }
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
                Err(error) => return Err(error),
            }
        }
        Err(io::Error::new(
            io::ErrorKind::AlreadyExists,
            "could not reserve a unique temporary settings filename",
        ))
    }
}

impl Drop for TemporaryFile {
    fn drop(&mut self) {
        drop(self.file.take());
        if !self.committed {
            // Never enumerate or clean up other writers' or earlier runs' files.
            let _ = fs::remove_file(&self.path);
        }
    }
}

#[cfg(windows)]
fn random_nonce() -> io::Result<u128> {
    use windows_sys::Win32::Security::Cryptography::{
        BCryptGenRandom, BCRYPT_USE_SYSTEM_PREFERRED_RNG,
    };
    let mut bytes = [0_u8; 16];
    // SAFETY: the system RNG accepts a null algorithm handle with this flag;
    // bytes is writable for the exact length supplied and lives through the call.
    let status = unsafe {
        BCryptGenRandom(
            std::ptr::null_mut(),
            bytes.as_mut_ptr(),
            bytes.len() as u32,
            BCRYPT_USE_SYSTEM_PREFERRED_RNG,
        )
    };
    if status < 0 {
        return Err(io::Error::other(format!(
            "system random generator failed (NTSTATUS {status:#x})"
        )));
    }
    Ok(u128::from_ne_bytes(bytes))
}

#[cfg(not(windows))]
fn random_nonce() -> io::Result<u128> {
    let mut bytes = [0_u8; 16];
    File::open("/dev/urandom")?.read_exact(&mut bytes)?;
    Ok(u128::from_ne_bytes(bytes))
}

#[cfg(windows)]
fn replace(source: &Path, destination: &Path) -> io::Result<()> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Foundation::{
        ERROR_ACCESS_DENIED, ERROR_LOCK_VIOLATION, ERROR_SHARING_VIOLATION,
    };
    use windows_sys::Win32::Storage::FileSystem::{
        MoveFileExW, MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH,
    };

    fn wide(path: &Path) -> io::Result<Vec<u16>> {
        let mut value: Vec<u16> = path.as_os_str().encode_wide().collect();
        if value.contains(&0) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "settings path contains a NUL character",
            ));
        }
        value.push(0);
        Ok(value)
    }
    let source = wide(source)?;
    let destination = wide(destination)?;
    // Do not enable MOVEFILE_COPY_ALLOWED: both paths are in the same directory
    // and replacement must remain a rename, not a non-atomic copy/delete.
    // Unlike ReplaceFileW, this also handles a destination that does not yet
    // exist, without an existence-check race or its partial merge failure modes.
    // https://learn.microsoft.com/windows/win32/api/winbase/nf-winbase-movefileexw
    // Concurrent Windows renames/readers or antivirus scanners can briefly hold
    // incompatible handles. Retry only access/sharing conflicts, using the same
    // already-synced temporary file. Permanent errors remain non-destructive.
    let mut attempt = 0;
    loop {
        // SAFETY: both buffers are valid, NUL-terminated UTF-16 and live until return.
        let result = unsafe {
            MoveFileExW(
                source.as_ptr(),
                destination.as_ptr(),
                MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
            )
        };
        if result != 0 {
            return Ok(());
        }
        let error = io::Error::last_os_error();
        let retryable = matches!(
            error.raw_os_error().map(|code| code as u32),
            Some(ERROR_ACCESS_DENIED | ERROR_LOCK_VIOLATION | ERROR_SHARING_VIOLATION)
        );
        if !retryable || attempt == 7 {
            return Err(error);
        }
        std::thread::sleep(std::time::Duration::from_millis(1 << attempt));
        attempt += 1;
    }
}

#[cfg(not(windows))]
fn replace(source: &Path, destination: &Path) -> io::Result<()> {
    fs::rename(source, destination)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::TestDirectory;

    #[test]
    fn partial_write_failure_preserves_previous_file_and_cleans_only_owned_temp() {
        let directory = TestDirectory::new();
        let path = directory.path().join("settings.json");
        save_atomic(&path, &Settings::default()).unwrap();
        let previous = fs::read(&path).unwrap();
        let unrelated = directory.path().join(".taskbarx-settings-unrelated.tmp");
        fs::write(&unrelated, b"another writer").unwrap();

        let error = atomic_write(&path, |file| {
            file.write_all(b"{\"schema_version\":")?;
            Err(io::Error::other("injected disk-full write failure"))
        })
        .unwrap_err();
        assert!(error.contains("write and flush"), "{error}");
        assert_eq!(fs::read(&path).unwrap(), previous);
        assert_eq!(fs::read(&unrelated).unwrap(), b"another writer");
        assert_eq!(fs::read_dir(directory.path()).unwrap().count(), 2);
    }

    #[test]
    fn temporary_files_are_unique_exclusively_created_and_cleaned_on_drop() {
        let directory = TestDirectory::new();
        let first = TemporaryFile::create(directory.path()).unwrap();
        let second = TemporaryFile::create(directory.path()).unwrap();
        assert_ne!(first.path, second.path);
        assert_eq!(first.path.parent(), Some(directory.path()));
        assert!(OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&first.path)
            .is_err());
        drop(first);
        assert!(second.path.exists());
        drop(second);
        assert_eq!(fs::read_dir(directory.path()).unwrap().count(), 0);
    }
}
