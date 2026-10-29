use crate::LaunchSpec;
use std::path::Path;

/// Create an explicitly requested .lnk for the sole `start-menu` action. No
/// default directory creation, execution, Start-folder installation or pinning.
pub fn create_start_shortcut(
    path: &Path,
    launch: &LaunchSpec,
    icon: Option<&Path>,
) -> Result<(), String> {
    #[cfg(windows)]
    {
        let sid = crate::native::user_sid()?;
        let _lock = crate::native::OperationLock::acquire(&format!("shortcut.{sid}"))?;
        let mut links = windows::LinksCom::new()?;
        protocol::create(&mut links, &sid, path, launch, icon)
    }
    #[cfg(not(windows))]
    {
        let _ = (path, launch, icon);
        Err(crate::unsupported())
    }
}

/// Remove only the specified owned Start-action link. Absence is idempotent.
pub fn remove_start_shortcut(path: &Path) -> Result<(), String> {
    #[cfg(windows)]
    {
        let sid = crate::native::user_sid()?;
        let _lock = crate::native::OperationLock::acquire(&format!("shortcut.{sid}"))?;
        let mut links = windows::LinksCom::new()?;
        protocol::remove(&mut links, &sid, path)
    }
    #[cfg(not(windows))]
    {
        let _ = path;
        Err(crate::unsupported())
    }
}

#[cfg(windows)]
#[path = "shortcut_windows.rs"]
mod windows;

#[cfg(any(windows, test))]
mod protocol {
    use crate::{path_text, validate_launch, LaunchSpec};
    use std::path::{Path, PathBuf};

    #[derive(Debug, Clone, PartialEq, Eq)]
    pub(super) struct Link {
        pub marker: String,
        pub launch: LaunchSpec,
        pub icon: Option<PathBuf>,
    }

    pub(super) trait Links {
        /// Must not resolve/run a link or follow a directory/reparse entry.
        fn read(&mut self, path: &Path) -> Result<Option<Link>, String>;
        /// Temporary owns cleanup on every error path and on Drop.
        type Temporary;
        fn stage(&mut self, path: &Path, link: &Link) -> Result<Self::Temporary, String>;
        /// Same-directory atomic publish; recheck the previous link first.
        fn publish(
            &mut self,
            temp: &mut Self::Temporary,
            path: &Path,
            previous: Option<&Link>,
        ) -> Result<(), String>;
        fn delete(&mut self, path: &Path, expected: &Link) -> Result<(), String>;
    }

    pub(super) fn destination(path: &Path) -> Result<(), String> {
        path_text(path)?;
        if !path
            .extension()
            .is_some_and(|e| e.eq_ignore_ascii_case("lnk"))
        {
            return Err("shortcut destination must have a .lnk extension".into());
        }
        Ok(())
    }

    pub(super) fn marker(sid: &str, launch: &LaunchSpec) -> Result<String, String> {
        validate_launch(launch)?;
        // A stable fingerprint binds the ownership marker to the target/action
        // and working directory. This detects accidental edits; it is NOT an
        // authentication boundary against another program running as this user.
        let mut hash = 0xcbf29ce484222325u64;
        let mut strings = vec![
            path_text(&launch.executable)?
                .replace('/', "\\")
                .to_lowercase(),
            path_text(&launch.working_directory)?
                .replace('/', "\\")
                .to_lowercase(),
        ];
        strings.extend(launch.args.clone());
        for string in strings {
            for byte in string.bytes().chain(Some(0)) {
                hash ^= u64::from(byte);
                hash = hash.wrapping_mul(0x100000001b3);
            }
        }
        Ok(format!(
            "TaskbarX-Rust owned Start link v1; sid={sid}; target={hash:016x}"
        ))
    }

    fn owned(link: &Link, sid: &str) -> bool {
        link.launch.args == ["start-menu"]
            && marker(sid, &link.launch).is_ok_and(|m| m == link.marker)
    }

    pub(super) fn create(
        links: &mut impl Links,
        sid: &str,
        path: &Path,
        launch: &LaunchSpec,
        icon: Option<&Path>,
    ) -> Result<(), String> {
        destination(path)?;
        validate_launch(launch)?;
        if launch.args != ["start-menu"] {
            return Err("Start shortcut must target only the start-menu action".into());
        }
        if let Some(icon) = icon {
            path_text(icon)?;
        }
        let desired = Link {
            marker: marker(sid, launch)?,
            launch: launch.clone(),
            icon: icon.map(Path::to_path_buf),
        };
        let before = links.read(path)?;
        if before.as_ref().is_some_and(|l| !owned(l, sid)) {
            return Err("refusing to overwrite an unrelated shortcut/destination".into());
        }
        if before.as_ref() == Some(&desired) {
            return Ok(());
        }
        let mut temp = links.stage(path, &desired)?;
        if links.read(path)? != before {
            return Err("shortcut changed while preparing; nothing replaced".into());
        }
        links.publish(&mut temp, path, before.as_ref())
    }

    pub(super) fn remove(links: &mut impl Links, sid: &str, path: &Path) -> Result<(), String> {
        destination(path)?;
        if let Some(link) = links.read(path)? {
            if !owned(&link, sid) {
                return Err(
                    "refusing to remove an unrelated shortcut or changed target/action".into(),
                );
            }
            links.delete(path, &link)?;
        }
        Ok(())
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        use std::{
            cell::RefCell,
            collections::BTreeMap,
            rc::Rc,
            sync::atomic::{AtomicU64, Ordering},
        };
        type Data = Rc<RefCell<(BTreeMap<PathBuf, Link>, Vec<String>)>>;
        struct Temp {
            path: PathBuf,
            link: Link,
            data: Data,
        }
        impl Drop for Temp {
            fn drop(&mut self) {
                let _ = std::fs::remove_file(&self.path);
                self.data.borrow_mut().1.push("temp-drop".into());
            }
        }
        #[derive(Default)]
        struct Fake {
            data: Data,
            fail_stage: bool,
            fail_publish: bool,
        }
        impl Links for Fake {
            type Temporary = Temp;
            fn read(&mut self, path: &Path) -> Result<Option<Link>, String> {
                Ok(self.data.borrow().0.get(path).cloned())
            }
            fn stage(&mut self, path: &Path, link: &Link) -> Result<Temp, String> {
                let temp = Temp {
                    path: path.with_extension("tmp"),
                    link: link.clone(),
                    data: self.data.clone(),
                };
                std::fs::write(&temp.path, "FAKE LINK fixture only").unwrap();
                self.data.borrow_mut().1.push("stage".into());
                if self.fail_stage {
                    return Err("fake COM save failed".into());
                }
                Ok(temp)
            }
            fn publish(
                &mut self,
                temp: &mut Temp,
                path: &Path,
                _: Option<&Link>,
            ) -> Result<(), String> {
                if self.fail_publish {
                    return Err("fake atomic replace failure".into());
                }
                self.data
                    .borrow_mut()
                    .0
                    .insert(path.into(), temp.link.clone());
                self.data.borrow_mut().1.push("publish".into());
                Ok(())
            }
            fn delete(&mut self, path: &Path, _: &Link) -> Result<(), String> {
                self.data.borrow_mut().0.remove(path);
                self.data.borrow_mut().1.push("delete".into());
                Ok(())
            }
        }
        struct Fixture(PathBuf);
        impl Fixture {
            fn new() -> Self {
                static N: AtomicU64 = AtomicU64::new(0);
                let root = std::env::temp_dir().join(format!(
                    "taskbarx-shell-fake-links-{}-{}",
                    std::process::id(),
                    N.fetch_add(1, Ordering::Relaxed)
                ));
                std::fs::create_dir(&root).unwrap();
                Self(root)
            }
            fn path(&self) -> PathBuf {
                self.0.join("fixture.lnk")
            }
        }
        impl Drop for Fixture {
            fn drop(&mut self) {
                std::fs::remove_dir(&self.0).unwrap();
            }
        }
        fn launch() -> LaunchSpec {
            let mut s = crate::tests::spec();
            s.args = vec!["start-menu".into()];
            s
        }
        #[test]
        fn target_icon_working_directory_and_idempotency() {
            let f = Fixture::new();
            let mut fake = Fake::default();
            let launch = launch();
            let icon = launch.working_directory.join("icon with spaces.ico");
            create(&mut fake, "sid", &f.path(), &launch, Some(&icon)).unwrap();
            let stored = fake.read(&f.path()).unwrap().unwrap();
            assert_eq!(stored.launch, launch);
            assert_eq!(stored.icon.as_ref(), Some(&icon));
            create(&mut fake, "sid", &f.path(), &launch, Some(&icon)).unwrap();
            assert_eq!(fake.data.borrow().1, ["stage", "publish", "temp-drop"]);
            remove(&mut fake, "sid", &f.path()).unwrap();
            remove(&mut fake, "sid", &f.path()).unwrap();
            assert_eq!(
                fake.data
                    .borrow()
                    .1
                    .iter()
                    .filter(|s| *s == "delete")
                    .count(),
                1
            );
        }
        #[test]
        fn collision_changed_target_or_wrong_action_is_never_removed() {
            let f = Fixture::new();
            let mut fake = Fake::default();
            let launch = launch();
            create(&mut fake, "sid", &f.path(), &launch, None).unwrap();
            for kind in 0..3 {
                let mut wrong = Link {
                    marker: marker("sid", &launch).unwrap(),
                    launch: launch.clone(),
                    icon: None,
                };
                match kind {
                    0 => wrong.marker = "foreign".into(),
                    1 => wrong.launch.args = vec!["run".into()],
                    _ => wrong.launch.executable = launch.working_directory.join("foreign.exe"),
                }
                fake.data.borrow_mut().0.insert(f.path(), wrong);
                assert!(remove(&mut fake, "sid", &f.path()).is_err());
                assert!(create(&mut fake, "sid", &f.path(), &launch, None).is_err());
            }
            assert!(!fake.data.borrow().1.iter().any(|c| c == "delete"));
        }
        #[test]
        fn temporary_cleanup_on_com_and_publish_failure_preserves_old_link() {
            let f = Fixture::new();
            let launch = launch();
            let mut fake = Fake::default();
            for stage in [true, false] {
                fake.fail_stage = stage;
                fake.fail_publish = !stage;
                assert!(create(&mut fake, "sid", &f.path(), &launch, None).is_err());
                assert_eq!(std::fs::read_dir(&f.0).unwrap().count(), 0);
                assert!(fake.read(&f.path()).unwrap().is_none());
            }
            fake.fail_publish = false;
            create(&mut fake, "sid", &f.path(), &launch, None).unwrap();
            let before = fake.read(&f.path()).unwrap();
            fake.fail_publish = true;
            assert!(create(
                &mut fake,
                "sid",
                &f.path(),
                &launch,
                Some(&launch.executable)
            )
            .is_err());
            assert_eq!(fake.read(&f.path()).unwrap(), before);
        }
        #[test]
        fn reject_non_start_or_relative_paths_before_effects() {
            let f = Fixture::new();
            let mut fake = Fake::default();
            assert!(create(&mut fake, "sid", &f.path(), &crate::tests::spec(), None).is_err());
            assert!(create(&mut fake, "sid", Path::new("relative.lnk"), &launch(), None).is_err());
            assert!(create(
                &mut fake,
                "sid",
                &f.path(),
                &launch(),
                Some(Path::new("relative.ico"))
            )
            .is_err());
            assert!(fake.data.borrow().1.is_empty());
        }
    }
}
