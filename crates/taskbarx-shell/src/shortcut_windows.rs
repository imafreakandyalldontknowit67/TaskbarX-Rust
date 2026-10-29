use super::protocol::{Link, Links};
use crate::{
    command_line,
    native::{decode, wide, Apartment},
    path_text, LaunchSpec,
};
use std::{
    fs::OpenOptions,
    path::{Path, PathBuf},
};
use windows::{
    core::{Interface, PCWSTR},
    Win32::{
        Storage::FileSystem::{
            MoveFileExW, FILE_ATTRIBUTE_REPARSE_POINT, MOVEFILE_REPLACE_EXISTING,
            MOVEFILE_WRITE_THROUGH,
        },
        System::Com::{
            CoCreateGuid, CoCreateInstance, IPersistFile, CLSCTX_INPROC_SERVER, STGM_READ,
        },
        UI::Shell::{IShellLinkW, ShellLink, SLGP_RAWPATH},
    },
};

pub(super) struct LinksCom {
    _apartment: Apartment,
}
impl LinksCom {
    pub(super) fn new() -> Result<Self, String> {
        Ok(Self {
            _apartment: Apartment::new()?,
        })
    }
}

pub(super) struct Temporary {
    path: Option<PathBuf>,
}
impl Drop for Temporary {
    fn drop(&mut self) {
        if let Some(path) = &self.path {
            for _ in 0..3 {
                match std::fs::remove_file(path) {
                    Ok(()) => break,
                    Err(e) if e.kind() == std::io::ErrorKind::NotFound => break,
                    Err(_) => {}
                }
            }
        }
    }
}

fn regular_file(path: &Path) -> Result<bool, String> {
    use std::os::windows::fs::MetadataExt;
    match std::fs::symlink_metadata(path) {
        Ok(m) if !m.is_file() || m.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT.0 != 0 => {
            Err("refusing a non-regular or reparse-point shortcut destination".into())
        }
        Ok(_) => Ok(true),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(e) => Err(format!("inspecting shortcut destination: {e}")),
    }
}

fn buffer(get: impl FnOnce(&mut [u16]) -> windows::core::Result<()>) -> Result<String, String> {
    let mut text = vec![0xffff; 32768];
    get(&mut text).map_err(|e| format!("reading shell link: {e}"))?;
    let length = text
        .iter()
        .position(|v| *v == 0)
        .ok_or("shell-link property was not terminated")?;
    if length == text.len() - 1 {
        return Err("shell-link property may have been truncated".into());
    }
    decode(&text[..length])
}

impl Links for LinksCom {
    type Temporary = Temporary;
    fn read(&mut self, path: &Path) -> Result<Option<Link>, String> {
        if !regular_file(path)? {
            return Ok(None);
        }
        let path = wide(path_text(path)?);
        unsafe {
            let link: IShellLinkW = CoCreateInstance(&ShellLink, None, CLSCTX_INPROC_SERVER)
                .map_err(|e| e.to_string())?;
            let persist: IPersistFile = link.cast().map_err(|e| e.to_string())?;
            // Load only; never Resolve (which can track/search/update links).
            persist
                .Load(PCWSTR(path.as_ptr()), STGM_READ)
                .map_err(|e| format!("destination is not a readable owned shell link: {e}"))?;
            let executable =
                buffer(|out| link.GetPath(out, std::ptr::null_mut(), SLGP_RAWPATH.0 as u32))?;
            let args = command_line::parse(&buffer(|out| link.GetArguments(out))?)?;
            let cwd = buffer(|out| link.GetWorkingDirectory(out))?;
            let marker = buffer(|out| link.GetDescription(out))?;
            let mut index = 0;
            let icon = buffer(|out| link.GetIconLocation(out, &mut index))?;
            if index != 0 {
                return Err("shortcut icon index was changed; refusing ownership".into());
            }
            Ok(Some(Link {
                marker,
                launch: LaunchSpec {
                    executable: executable.into(),
                    args,
                    working_directory: cwd.into(),
                },
                icon: (!icon.is_empty()).then(|| icon.into()),
            }))
        }
    }

    fn stage(&mut self, path: &Path, value: &Link) -> Result<Temporary, String> {
        let parent = path.parent().ok_or("shortcut destination has no parent")?;
        if !parent.is_dir() {
            return Err(
                "shortcut parent directory must already exist; no default folders are created"
                    .into(),
            );
        }
        // Shell Link path fields are bounded by MAX_PATH in the documented
        // interface. Reject rather than silently truncate a long target/icon.
        for p in [
            Some(value.launch.executable.as_path()),
            Some(value.launch.working_directory.as_path()),
            value.icon.as_deref(),
        ]
        .into_iter()
        .flatten()
        {
            if path_text(p)?.encode_utf16().count() >= 260 {
                return Err("Shell Link target/directory/icon paths of 260 UTF-16 units or longer are unsupported".into());
            }
            if path_text(p)?.contains('%') {
                return Err("Shell Link environment-variable paths are unsupported; use literal absolute paths".into());
            }
        }
        let mut temp = None;
        for _ in 0..32 {
            let id = unsafe { CoCreateGuid() }
                .map_err(|e| format!("generating temporary shortcut identity: {e}"))?;
            let candidate = parent.join(format!(".taskbarx-start-{id:?}.lnk"));
            match OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&candidate)
            {
                Ok(file) => {
                    drop(file);
                    temp = Some(Temporary {
                        path: Some(candidate),
                    });
                    break;
                }
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(e) => return Err(format!("reserving same-directory temporary shortcut: {e}")),
            }
        }
        let temp = temp.ok_or("could not reserve a temporary shortcut after 32 attempts")?;
        let temp_path = temp
            .path
            .as_deref()
            .ok_or("temporary shortcut path missing")?;
        unsafe {
            let link: IShellLinkW = CoCreateInstance(&ShellLink, None, CLSCTX_INPROC_SERVER)
                .map_err(|e| e.to_string())?;
            let exe = wide(path_text(&value.launch.executable)?);
            let cwd = wide(path_text(&value.launch.working_directory)?);
            let args = wide(&command_line::arguments(&value.launch.args));
            let description = wide(&value.marker);
            link.SetPath(PCWSTR(exe.as_ptr()))
                .map_err(|e| format!("setting shortcut target: {e}"))?;
            link.SetArguments(PCWSTR(args.as_ptr()))
                .map_err(|e| format!("setting shortcut arguments: {e}"))?;
            link.SetWorkingDirectory(PCWSTR(cwd.as_ptr()))
                .map_err(|e| e.to_string())?;
            link.SetDescription(PCWSTR(description.as_ptr()))
                .map_err(|e| e.to_string())?;
            if let Some(icon) = &value.icon {
                let icon = wide(path_text(icon)?);
                link.SetIconLocation(PCWSTR(icon.as_ptr()), 0)
                    .map_err(|e| e.to_string())?;
            }
            let persist: IPersistFile = link.cast().map_err(|e| e.to_string())?;
            let target = wide(path_text(temp_path)?);
            persist
                .Save(PCWSTR(target.as_ptr()), true)
                .map_err(|e| format!("saving temporary shortcut: {e}"))?;
            // Interfaces drop before re-opening the saved link below.
        }
        if self.read(temp_path)?.as_ref() != Some(value) {
            return Err("saved shell link differs from supplied target/arguments/directory/icon; nothing published".into());
        }
        Ok(temp)
    }

    fn publish(
        &mut self,
        temp: &mut Temporary,
        path: &Path,
        previous: Option<&Link>,
    ) -> Result<(), String> {
        if self.read(path)?.as_ref() != previous {
            return Err("shortcut changed before atomic publish; nothing replaced".into());
        }
        let source = wide(path_text(
            temp.path
                .as_deref()
                .ok_or("temporary shortcut already published")?,
        )?);
        let destination = wide(path_text(path)?);
        unsafe {
            // Same-directory rename only (never COPY_ALLOWED). Unlike
            // ReplaceFileW without backup, this does not have its documented
            // partial metadata-merge failures that can remove the old path.
            // New links omit REPLACE_EXISTING so a collision is never replaced.
            let flags = if previous.is_some() {
                MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH
            } else {
                MOVEFILE_WRITE_THROUGH
            };
            MoveFileExW(PCWSTR(source.as_ptr()), PCWSTR(destination.as_ptr()), flags)
                .map_err(|e| format!("atomic shortcut publish failed: {e}"))?;
        }
        // The temporary name is no longer ours. Never delete a new entry
        // another caller might create there after the successful rename.
        temp.path = None;
        Ok(())
    }

    fn delete(&mut self, path: &Path, expected: &Link) -> Result<(), String> {
        match self.read(path)? {
            None => Ok(()),
            Some(link) if &link == expected => std::fs::remove_file(path)
                .map_err(|e| format!("removing specified owned shortcut: {e}")),
            Some(_) => Err("shortcut changed before deletion; nothing removed".into()),
        }
    }
}
