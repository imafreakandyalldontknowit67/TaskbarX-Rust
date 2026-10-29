use crate::runtime::{self, Backend, Control, NativeBackend};
use taskbarx_config::Settings;

mod protocol;
#[cfg(windows)]
mod windows;

/// Acquiring a lease is the ONLY gateway to creating a mutating backend.
/// The lease must outlive backend restoration, backend Drop, and COM cleanup.
trait Coordinator {
    type Lease: Control;
    fn claim(&mut self) -> Result<Option<Self::Lease>, String>;
    fn stop_existing(&mut self, timeout_ms: u32) -> Result<(), String>;
}

fn start<C: Coordinator, B: Backend>(
    coordinator: &mut C,
    factory: impl FnOnce() -> Result<B, String>,
    settings: &Settings,
) -> Result<(), String> {
    settings.validate()?;
    let mut lease = coordinator.claim()?.ok_or_else(|| {
        "taskbarx is already running for this user and session; use `taskbarx stop` first"
            .to_owned()
    })?;
    let backend = factory()?;
    runtime::drive(backend, &mut lease, settings)
}

pub(crate) fn run(settings: &Settings) -> Result<(), String> {
    #[cfg(windows)]
    {
        start(
            &mut windows::NamedCoordinator::new()?,
            NativeBackend::new,
            settings,
        )
    }
    #[cfg(not(windows))]
    {
        let _ = settings;
        Err("The taskbar runtime requires Windows.".into())
    }
}

pub(crate) fn stop(timeout_ms: u32) -> Result<(), String> {
    #[cfg(windows)]
    {
        windows::NamedCoordinator::new()?.stop_existing(timeout_ms)
    }
    #[cfg(not(windows))]
    {
        let _ = timeout_ms;
        Err("The taskbar runtime requires Windows.".into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{cell::RefCell, rc::Rc};
    use taskbarx_win32::{Discovery, Point, Style, TaskbarId};
    type Log = Rc<RefCell<Vec<&'static str>>>;
    struct Lease(Log);
    impl Control for Lease {
        fn now_ms(&self) -> u64 {
            0
        }
        fn stopped(&mut self) -> Result<bool, String> {
            Ok(true)
        }
        fn wait(&mut self, _: u64) -> Result<(), String> {
            panic!("not expected")
        }
    }
    impl Drop for Lease {
        fn drop(&mut self) {
            self.0.borrow_mut().push("lease released");
        }
    }
    struct FakeCoordinator {
        duplicate: bool,
        log: Log,
    }
    impl Coordinator for FakeCoordinator {
        type Lease = Lease;
        fn claim(&mut self) -> Result<Option<Lease>, String> {
            self.log.borrow_mut().push("claim");
            Ok((!self.duplicate).then(|| Lease(self.log.clone())))
        }
        fn stop_existing(&mut self, _: u32) -> Result<(), String> {
            self.log.borrow_mut().push("check existing only");
            Ok(())
        }
    }
    struct FakeBackend(Log);
    impl Backend for FakeBackend {
        fn discover(&mut self) -> Result<Discovery, String> {
            panic!("stopped before discovery")
        }
        fn move_list(&mut self, _: &TaskbarId, _: Point) -> Result<(), String> {
            panic!()
        }
        fn restore_position(&mut self, _: &TaskbarId) -> Result<(), String> {
            panic!()
        }
        fn set_style(&mut self, _: &TaskbarId, _: Style, _: [u8; 4]) -> Result<(), String> {
            panic!()
        }
        fn restore(&mut self) -> Vec<String> {
            self.0.borrow_mut().push("restore");
            vec![]
        }
    }
    impl Drop for FakeBackend {
        fn drop(&mut self) {
            self.0.borrow_mut().push("backend dropped");
        }
    }
    #[test]
    fn duplicate_never_creates_backend_or_resets_control() {
        let log = Log::default();
        let mut c = FakeCoordinator {
            duplicate: true,
            log: log.clone(),
        };
        let result = start::<_, FakeBackend>(
            &mut c,
            || panic!("must not create session"),
            &Settings::default(),
        );
        assert!(result.unwrap_err().contains("already running"));
        assert_eq!(*log.borrow(), ["claim"]);
    }
    #[test]
    fn cleanup_and_backend_drop_precede_instance_release() {
        let log = Log::default();
        let mut c = FakeCoordinator {
            duplicate: false,
            log: log.clone(),
        };
        start(
            &mut c,
            || Ok(FakeBackend(log.clone())),
            &Settings::default(),
        )
        .unwrap();
        assert_eq!(
            *log.borrow(),
            ["claim", "restore", "backend dropped", "lease released"]
        );
    }
    #[test]
    fn absent_stop_does_not_construct_session_or_claim_runtime() {
        let log = Log::default();
        FakeCoordinator {
            duplicate: false,
            log: log.clone(),
        }
        .stop_existing(100)
        .unwrap();
        assert_eq!(*log.borrow(), ["check existing only"]);
    }
    #[test]
    fn failed_session_creation_releases_instance() {
        let log = Log::default();
        let mut c = FakeCoordinator {
            duplicate: false,
            log: log.clone(),
        };
        let result = start::<_, FakeBackend>(
            &mut c,
            || Err("COM unavailable".into()),
            &Settings::default(),
        );
        assert_eq!(result.unwrap_err(), "COM unavailable");
        assert_eq!(*log.borrow(), ["claim", "lease released"]);
    }
}
