//! An owned, owner-thread notification icon. Callbacks only queue intentions;
//! the caller decides how to open settings, restart, or stop the runtime.
use std::{marker::PhantomData, rc::Rc};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrayAction {
    OpenSettings,
    Restart,
    Stop,
}

/// Must be polled and closed on its creating thread (`!Send` and `!Sync`).
/// No worker thread, process execution, or runtime-stop signaling is involved.
pub struct TrayIcon {
    #[cfg(windows)]
    controller: protocol::Controller<native::Native>,
    _owner_thread: PhantomData<Rc<()>>,
}

impl TrayIcon {
    pub fn new() -> Result<Self, String> {
        // Unit tests must never place a real icon on the user's desktop, even
        // if a future test accidentally invokes this public constructor.
        if cfg!(test) {
            return Err("native tray creation is disabled in unit tests".into());
        }
        #[cfg(windows)]
        {
            let inbox = Rc::new(std::cell::RefCell::new(protocol::Inbox::default()));
            let backend = native::Native::new(inbox.clone())?;
            Ok(Self {
                controller: protocol::Controller::new(backend, inbox)?,
                _owner_thread: PhantomData,
            })
        }
        #[cfg(not(windows))]
        Err("notification-area icons are only supported on Windows".into())
    }

    /// Dispatches at most 64 messages for this icon's own window. It never
    /// consumes thread-level WM_QUIT or messages belonging to other windows.
    /// Repeated identical actions within a poll are coalesced (at most three).
    pub fn poll_actions(&mut self) -> Result<Vec<TrayAction>, String> {
        #[cfg(windows)]
        {
            self.controller.poll_actions()
        }
        #[cfg(not(windows))]
        Err("notification-area icons are only supported on Windows".into())
    }

    /// Idempotent. Failed releases retain ownership for a subsequent close;
    /// Drop makes another bounded best-effort attempt.
    pub fn close(&mut self) -> Result<(), String> {
        #[cfg(windows)]
        {
            self.controller.close()
        }
        #[cfg(not(windows))]
        Err("notification-area icons are only supported on Windows".into())
    }
}

impl Drop for TrayIcon {
    fn drop(&mut self) {
        let _ = self.close();
    }
}

#[cfg(windows)]
#[path = "tray_windows.rs"]
mod native;

#[cfg(any(windows, test))]
mod protocol {
    use super::TrayAction;
    use std::{cell::RefCell, collections::VecDeque, rc::Rc};

    pub(super) const SETTINGS: u16 = 1;
    pub(super) const RESTART: u16 = 2;
    pub(super) const STOP: u16 = 3;
    pub(super) const MESSAGE_BUDGET: usize = 64;
    const RELEASE_ATTEMPTS: usize = 3;
    const RECOVERY_DELAY_MS: u64 = 500;

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub(super) struct MenuAnchor {
        pub x: i32,
        pub y: i32,
    }

    #[derive(Debug, Clone, Copy)]
    pub(super) enum Callback {
        Activate,
        Menu(MenuAnchor),
        MenuClosed,
        Command(u16),
        TaskbarCreated,
    }

    #[derive(Default)]
    pub(super) struct Inbox {
        actions: VecDeque<TrayAction>,
        menu: Option<MenuAnchor>,
        menu_closed: bool,
        taskbar_created: bool,
        pub closing: bool,
    }

    impl Inbox {
        pub fn receive(&mut self, callback: Callback) {
            if self.closing {
                return;
            }
            let action = match callback {
                Callback::Activate | Callback::Command(SETTINGS) => Some(TrayAction::OpenSettings),
                Callback::Command(RESTART) => Some(TrayAction::Restart),
                Callback::Command(STOP) => Some(TrayAction::Stop),
                Callback::Menu(anchor) => {
                    self.menu = Some(anchor);
                    None
                }
                Callback::MenuClosed => {
                    self.menu_closed = true;
                    None
                }
                Callback::TaskbarCreated => {
                    self.taskbar_created = true;
                    None
                }
                Callback::Command(_) => None,
            };
            if let Some(action) = action {
                // Three distinct actions maximum: flooding cannot displace Stop.
                if !self.actions.contains(&action) {
                    self.actions.push_back(action);
                }
            }
        }
    }

    /// Narrow adapter: every operation concerns resources owned by this tray.
    pub(super) trait Backend {
        fn register_class(&mut self) -> Result<(), String>;
        fn create_window(&mut self) -> Result<(), String>;
        fn copy_icon(&mut self) -> Result<(), String>;
        fn create_menu(&mut self) -> Result<(), String>;
        fn configure_menu(&mut self) -> Result<(), String>;
        fn add_icon(&mut self) -> Result<(), String>;
        fn modify_icon(&mut self) -> Result<(), String>;
        fn set_version(&mut self) -> Result<(), String>;
        fn show_menu(&mut self, anchor: MenuAnchor) -> Result<(), String>;
        fn return_focus(&mut self) -> Result<(), String>;
        fn pump_messages(&mut self, budget: usize);
        fn now_ms(&self) -> u64;
        fn delete_icon(&mut self) -> Result<(), String>;
        fn destroy_menu(&mut self) -> Result<(), String>;
        fn destroy_window(&mut self) -> Result<(), String>;
        fn destroy_icon(&mut self) -> Result<(), String>;
        fn unregister_class(&mut self) -> Result<(), String>;
    }

    #[derive(Default)]
    struct Owned {
        class: bool,
        window: bool,
        icon: bool,
        menu: bool,
        notification: bool,
    }

    pub(super) struct Controller<B: Backend> {
        backend: B,
        inbox: Rc<RefCell<Inbox>>,
        owned: Owned,
        closing: bool,
        recover: bool,
        next_recovery_ms: u64,
    }

    impl<B: Backend> Controller<B> {
        pub fn new(backend: B, inbox: Rc<RefCell<Inbox>>) -> Result<Self, String> {
            let mut this = Self {
                backend,
                inbox,
                owned: Owned::default(),
                closing: false,
                recover: false,
                next_recovery_ms: 0,
            };
            // Mark ownership immediately, so every partially initialized path
            // uses exactly the same retryable cleanup protocol as normal exit.
            this.backend.register_class()?;
            this.owned.class = true;
            this.backend.create_window()?;
            this.owned.window = true;
            this.backend.copy_icon()?;
            this.owned.icon = true;
            this.backend.create_menu()?;
            this.owned.menu = true;
            this.backend.configure_menu()?;
            this.backend.add_icon()?;
            this.owned.notification = true;
            this.backend.set_version()?;
            Ok(this)
        }

        pub fn poll_actions(&mut self) -> Result<Vec<TrayAction>, String> {
            if self.closing {
                return Ok(Vec::new());
            }
            self.backend.pump_messages(MESSAGE_BUDGET);
            let menu_closed = std::mem::take(&mut self.inbox.borrow_mut().menu_closed);
            if menu_closed {
                // ESC/dismiss should restore keyboard navigation in the tray.
                // Best effort: Explorer disappearing must not hide user actions.
                let _ = self.backend.return_focus();
            }
            {
                let mut inbox = self.inbox.borrow_mut();
                if std::mem::take(&mut inbox.taskbar_created) {
                    self.recover = true;
                }
                // User controls must not be starved by a missing/restarting
                // Explorer or by a failed menu operation.
                if !inbox.actions.is_empty() {
                    return Ok(inbox.actions.drain(..).collect());
                }
            }
            if self.recover && self.backend.now_ms() >= self.next_recovery_ms {
                self.next_recovery_ms = self.backend.now_ms().saturating_add(RECOVERY_DELAY_MS);
                // MODIFY handles duplicate broadcasts without adding a second
                // icon. A real Explorer restart requires ADD + SETVERSION.
                if self.backend.modify_icon().is_err() {
                    // A failed MODIFY is not proof that our old notification
                    // vanished. Keep ownership if ADD also fails, so cleanup
                    // still attempts NIM_DELETE rather than abandoning it.
                    self.backend.add_icon()?;
                    self.owned.notification = true;
                }
                self.backend.set_version()?;
                self.recover = false;
            }
            let menu = self.inbox.borrow_mut().menu.take();
            if let Some(anchor) = menu {
                self.backend.show_menu(anchor)?;
            }
            Ok(self.inbox.borrow_mut().actions.drain(..).collect())
        }

        pub fn close(&mut self) -> Result<(), String> {
            self.closing = true;
            {
                let mut inbox = self.inbox.borrow_mut();
                inbox.closing = true;
                inbox.actions.clear();
                inbox.menu = None;
                inbox.menu_closed = false;
            }
            let mut errors = Vec::new();
            release(
                &mut self.owned.notification,
                || self.backend.delete_icon(),
                &mut errors,
            );
            release(
                &mut self.owned.menu,
                || self.backend.destroy_menu(),
                &mut errors,
            );
            // Retain the callback HWND and HICON if the shell still owns its
            // notification. Retain the class until its HWND has gone away.
            if !self.owned.notification {
                if !self.owned.menu {
                    release(
                        &mut self.owned.window,
                        || self.backend.destroy_window(),
                        &mut errors,
                    );
                }
                release(
                    &mut self.owned.icon,
                    || self.backend.destroy_icon(),
                    &mut errors,
                );
            }
            if !self.owned.window {
                release(
                    &mut self.owned.class,
                    || self.backend.unregister_class(),
                    &mut errors,
                );
            }
            if errors.is_empty() {
                Ok(())
            } else {
                Err(errors.join("; "))
            }
        }
    }

    fn release(
        owned: &mut bool,
        mut operation: impl FnMut() -> Result<(), String>,
        errors: &mut Vec<String>,
    ) {
        if !*owned {
            return;
        }
        let mut last = String::new();
        for _ in 0..RELEASE_ATTEMPTS {
            match operation() {
                Ok(()) => {
                    *owned = false;
                    return;
                }
                Err(error) => last = error,
            }
        }
        errors.push(last);
    }

    impl<B: Backend> Drop for Controller<B> {
        fn drop(&mut self) {
            let _ = self.close();
        }
    }

    /// Final native-owner fallback: unlike retryable close, no later caller can
    /// reuse the resources. Attempt every release independently so an absent
    /// Explorer cannot strand our HWND/HICON/menu/class. Adapter releases must
    /// be no-ops for resources no longer owned.
    pub(super) fn terminal_cleanup<B: Backend>(backend: &mut B) {
        for operation in [
            B::delete_icon as fn(&mut B) -> Result<(), String>,
            B::destroy_menu,
            B::destroy_window,
            B::destroy_icon,
            B::unregister_class,
        ] {
            for _ in 0..RELEASE_ATTEMPTS {
                if operation(backend).is_ok() {
                    break;
                }
            }
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        use std::collections::HashMap;

        #[derive(Default)]
        struct State {
            calls: Vec<&'static str>,
            failures: HashMap<&'static str, usize>,
            present: bool,
            now: u64,
            budget: usize,
        }
        struct Fake(Rc<RefCell<State>>);
        impl Fake {
            fn call(&self, name: &'static str) -> Result<(), String> {
                let mut state = self.0.borrow_mut();
                state.calls.push(name);
                let failures = state.failures.entry(name).or_default();
                if *failures > 0 {
                    *failures -= 1;
                    Err(format!("failed {name}"))
                } else {
                    Ok(())
                }
            }
        }
        macro_rules! operation {
            ($($name:ident),*) => {$(
                fn $name(&mut self) -> Result<(), String> { self.call(stringify!($name)) }
            )*};
        }
        impl Backend for Fake {
            operation!(
                register_class,
                create_window,
                copy_icon,
                create_menu,
                configure_menu,
                set_version,
                return_focus,
                destroy_menu,
                destroy_window,
                destroy_icon,
                unregister_class
            );
            fn add_icon(&mut self) -> Result<(), String> {
                self.call("add_icon")?;
                assert!(!self.0.borrow().present, "duplicate notification icon");
                self.0.borrow_mut().present = true;
                Ok(())
            }
            fn modify_icon(&mut self) -> Result<(), String> {
                self.call("modify_icon")?;
                if self.0.borrow().present {
                    Ok(())
                } else {
                    Err("Explorer replaced".into())
                }
            }
            fn delete_icon(&mut self) -> Result<(), String> {
                self.call("delete_icon")?;
                self.0.borrow_mut().present = false;
                Ok(())
            }
            fn show_menu(&mut self, _: MenuAnchor) -> Result<(), String> {
                self.call("show_menu")
            }
            fn pump_messages(&mut self, budget: usize) {
                self.0.borrow_mut().budget = budget;
            }
            fn now_ms(&self) -> u64 {
                self.0.borrow().now
            }
        }
        fn setup() -> (Controller<Fake>, Rc<RefCell<Inbox>>, Rc<RefCell<State>>) {
            let state = Rc::new(RefCell::new(State::default()));
            let inbox = Rc::new(RefCell::new(Inbox::default()));
            let controller = Controller::new(Fake(state.clone()), inbox.clone()).unwrap();
            state.borrow_mut().calls.clear();
            (controller, inbox, state)
        }

        #[test]
        fn callbacks_only_enqueue_bounded_deduplicated_intentions() {
            let (mut tray, inbox, state) = setup();
            for _ in 0..1000 {
                let mut inbox = inbox.borrow_mut();
                inbox.receive(Callback::Activate);
                inbox.receive(Callback::Command(SETTINGS));
                inbox.receive(Callback::Command(RESTART));
                inbox.receive(Callback::Command(STOP));
                inbox.receive(Callback::Command(999));
            }
            assert!(state.borrow().calls.is_empty());
            assert_eq!(
                tray.poll_actions().unwrap(),
                vec![
                    TrayAction::OpenSettings,
                    TrayAction::Restart,
                    TrayAction::Stop
                ]
            );
            assert_eq!(state.borrow().budget, 64);
            assert!(tray.poll_actions().unwrap().is_empty());
        }

        #[test]
        fn duplicate_context_requests_show_one_menu() {
            let (mut tray, inbox, state) = setup();
            for _ in 0..1000 {
                inbox
                    .borrow_mut()
                    .receive(Callback::Menu(MenuAnchor { x: -50, y: 100 }));
            }
            tray.poll_actions().unwrap();
            assert_eq!(state.borrow().calls, vec!["show_menu"]);
        }

        #[test]
        fn menu_dismiss_restores_keyboard_focus_without_starving_actions() {
            let (mut tray, inbox, state) = setup();
            state.borrow_mut().failures.insert("return_focus", 1);
            inbox.borrow_mut().receive(Callback::MenuClosed);
            inbox.borrow_mut().receive(Callback::MenuClosed);
            inbox.borrow_mut().receive(Callback::Command(STOP));
            assert_eq!(tray.poll_actions().unwrap(), vec![TrayAction::Stop]);
            assert_eq!(state.borrow().calls, vec!["return_focus"]);
        }

        #[test]
        fn explorer_recovery_and_duplicate_broadcasts_do_not_duplicate_icons() {
            let (mut tray, inbox, state) = setup();
            state.borrow_mut().present = false;
            for _ in 0..10 {
                inbox.borrow_mut().receive(Callback::TaskbarCreated);
            }
            tray.poll_actions().unwrap();
            assert_eq!(
                state.borrow().calls,
                vec!["modify_icon", "add_icon", "set_version"]
            );
            state.borrow_mut().calls.clear();
            state.borrow_mut().now = 500;
            inbox.borrow_mut().receive(Callback::TaskbarCreated);
            tray.poll_actions().unwrap();
            assert_eq!(state.borrow().calls, vec!["modify_icon", "set_version"]);
        }

        #[test]
        fn failed_recovery_is_rate_limited_and_stop_is_not_starved() {
            let (mut tray, inbox, state) = setup();
            state.borrow_mut().present = false;
            state.borrow_mut().failures.insert("add_icon", 1);
            inbox.borrow_mut().receive(Callback::TaskbarCreated);
            assert!(tray.poll_actions().is_err());
            state.borrow_mut().calls.clear();
            tray.poll_actions().unwrap();
            assert!(state.borrow().calls.is_empty());
            inbox.borrow_mut().receive(Callback::Command(STOP));
            assert_eq!(tray.poll_actions().unwrap(), vec![TrayAction::Stop]);
            state.borrow_mut().now = 500;
            tray.poll_actions().unwrap();
            assert!(state.borrow().present);
        }

        #[test]
        fn close_retries_keeps_failed_ownership_and_is_idempotent() {
            let (mut tray, inbox, state) = setup();
            state.borrow_mut().failures.insert("delete_icon", 4);
            assert!(tray.close().is_err());
            assert_eq!(
                state.borrow().calls,
                vec!["delete_icon", "delete_icon", "delete_icon", "destroy_menu"]
            );
            state.borrow_mut().calls.clear();
            tray.close().unwrap();
            assert_eq!(
                state.borrow().calls,
                vec![
                    "delete_icon",
                    "delete_icon",
                    "destroy_window",
                    "destroy_icon",
                    "unregister_class"
                ]
            );
            state.borrow_mut().calls.clear();
            tray.close().unwrap();
            inbox.borrow_mut().receive(Callback::Command(STOP));
            assert!(tray.poll_actions().unwrap().is_empty());
            assert!(state.borrow().calls.is_empty());
        }

        #[test]
        fn all_cleanup_failures_are_retryable_and_independent() {
            for failure in [
                "destroy_menu",
                "destroy_window",
                "destroy_icon",
                "unregister_class",
            ] {
                let (mut tray, _, state) = setup();
                state.borrow_mut().failures.insert(failure, 3);
                assert!(tray.close().is_err(), "{failure}");
                tray.close().unwrap();
                assert!(
                    !tray.owned.class
                        && !tray.owned.window
                        && !tray.owned.icon
                        && !tray.owned.menu
                        && !tray.owned.notification
                );
            }
        }

        #[test]
        fn every_partial_creation_path_cleans_owned_resources() {
            let creation = [
                "register_class",
                "create_window",
                "copy_icon",
                "create_menu",
                "configure_menu",
                "add_icon",
                "set_version",
            ];
            for (index, failure) in creation.iter().enumerate() {
                let state = Rc::new(RefCell::new(State::default()));
                state.borrow_mut().failures.insert(failure, 1);
                assert!(Controller::new(
                    Fake(state.clone()),
                    Rc::new(RefCell::new(Inbox::default()))
                )
                .is_err());
                let calls = &state.borrow().calls;
                assert_eq!(calls.contains(&"unregister_class"), index > 0, "{failure}");
                assert_eq!(calls.contains(&"destroy_window"), index > 1, "{failure}");
                assert_eq!(calls.contains(&"destroy_icon"), index > 2, "{failure}");
                assert_eq!(calls.contains(&"destroy_menu"), index > 3, "{failure}");
                assert_eq!(calls.contains(&"delete_icon"), index > 5, "{failure}");
            }
        }

        #[test]
        fn drop_releases_all_owned_resources() {
            let (tray, _, state) = setup();
            drop(tray);
            assert_eq!(
                state.borrow().calls,
                vec![
                    "delete_icon",
                    "destroy_menu",
                    "destroy_window",
                    "destroy_icon",
                    "unregister_class"
                ]
            );
        }

        #[test]
        fn terminal_cleanup_releases_local_resources_if_explorer_is_absent() {
            let state = Rc::new(RefCell::new(State::default()));
            state
                .borrow_mut()
                .failures
                .insert("delete_icon", usize::MAX);
            terminal_cleanup(&mut Fake(state.clone()));
            assert_eq!(
                state.borrow().calls,
                vec![
                    "delete_icon",
                    "delete_icon",
                    "delete_icon",
                    "destroy_menu",
                    "destroy_window",
                    "destroy_icon",
                    "unregister_class",
                ]
            );
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn native_constructor_is_disabled_in_unit_tests() {
        assert!(super::TrayIcon::new().is_err());
    }
}
