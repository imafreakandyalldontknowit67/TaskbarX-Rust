use crate::{LaunchSpec, StartupStatus};

/// Read only: no task/folder creation, migration, repair, or engine execution.
pub fn startup_status() -> Result<StartupStatus, String> {
    #[cfg(windows)]
    {
        let sid = crate::native::user_sid()?;
        let mut scheduler = windows::SchedulerCom::connect()?;
        protocol::status(&mut scheduler, &sid)
    }
    #[cfg(not(windows))]
    Err(crate::unsupported())
}

/// Explicitly register/update only our current-user interactive-logon task.
/// Does not run the action. Existing engines/settings/shortcuts are untouched.
pub fn enable_startup(spec: &LaunchSpec, delay_secs: u32) -> Result<StartupStatus, String> {
    crate::validate_launch(spec)?;
    if delay_secs > 86400 {
        return Err("startup delay must be in 0..=86400 seconds".into());
    }
    #[cfg(windows)]
    {
        let sid = crate::native::user_sid()?;
        let _lock = crate::native::OperationLock::acquire(&format!("startup.{sid}"))?;
        let mut scheduler = windows::SchedulerCom::connect()?;
        protocol::enable(&mut scheduler, &sid, spec, delay_secs)
    }
    #[cfg(not(windows))]
    Err(crate::unsupported())
}

/// Remove only our owned registration, not the active engine or any files.
pub fn disable_startup() -> Result<StartupStatus, String> {
    #[cfg(windows)]
    {
        let sid = crate::native::user_sid()?;
        let _lock = crate::native::OperationLock::acquire(&format!("startup.{sid}"))?;
        let mut scheduler = windows::SchedulerCom::connect()?;
        protocol::disable(&mut scheduler, &sid)
    }
    #[cfg(not(windows))]
    Err(crate::unsupported())
}

#[cfg(windows)]
#[path = "startup_windows.rs"]
mod windows;

#[cfg(any(windows, test))]
mod protocol {
    use crate::{command_line, path_text, validate_launch, LaunchSpec, StartupStatus};

    pub(super) const MARKER: &str = "TaskbarX-Rust owned startup v1";

    #[derive(Clone, Debug, PartialEq, Eq)]
    pub(super) struct Record {
        pub xml: String,
        pub marker: String,
        pub principal_sid: String,
        pub group: String,
        pub interactive: bool,
        pub least_privilege: bool,
        pub logon_sid: Option<String>,
        pub enabled: bool,
        pub launch: Option<LaunchSpec>,
        pub delay: Option<u32>,
        pub warnings: Vec<String>,
    }

    impl Record {
        pub(super) fn owned(&self, sid: &str) -> bool {
            self.marker == marker(sid)
                && self.principal_sid == sid
                && self.group.is_empty()
                && self.interactive
                && self.least_privilege
                && self.logon_sid.as_deref() == Some(sid)
        }
    }

    pub(super) trait Scheduler {
        fn read(&mut self, name: &str) -> Result<Option<Record>, String>;
        /// Schema/semantic validation only; must not register, execute, or delete.
        fn validate(&mut self, name: &str, xml: &str) -> Result<String, String>;
        /// create=true must fail on collision; false must update an existing task.
        /// Neither mode may execute an action, including registration triggers.
        fn write(&mut self, name: &str, xml: &str, create: bool) -> Result<(), String>;
        fn delete(&mut self, name: &str) -> Result<(), String>;
    }

    pub(super) fn marker(sid: &str) -> String {
        format!("{MARKER}; sid={sid}")
    }

    pub(super) fn task_name(sid: &str) -> Result<String, String> {
        if !sid.starts_with("S-1-")
            || sid.len() > 184
            || !sid
                .bytes()
                .all(|c| c.is_ascii_digit() || c == b'S' || c == b'-')
        {
            return Err("invalid token SID".into());
        }
        Ok(format!("TaskbarX-Rust.Startup.v1.{sid}"))
    }

    fn snapshot(name: String, sid: &str, record: Option<Record>) -> StartupStatus {
        match record {
            None => StartupStatus {
                registered: false,
                enabled: false,
                owned: false,
                task_name: name,
                launch: None,
                delay_secs: None,
                warnings: vec![],
            },
            Some(record) => {
                let owned = record.owned(sid);
                let mut warnings = record.warnings;
                if !owned {
                    warnings.push("conflicting registration: ownership/principal/logon validation failed; it will not be replaced or deleted".into());
                }
                if !record.enabled {
                    warnings.push("registration is disabled".into());
                }
                StartupStatus {
                    registered: true,
                    enabled: record.enabled,
                    owned,
                    task_name: name,
                    launch: record.launch,
                    delay_secs: record.delay,
                    warnings,
                }
            }
        }
    }

    pub(super) fn status(
        scheduler: &mut impl Scheduler,
        sid: &str,
    ) -> Result<StartupStatus, String> {
        let name = task_name(sid)?;
        let record = scheduler
            .read(&name)
            .map_err(|e| format!("startup status inaccessible/unknown (not absent): {e}"))?;
        Ok(snapshot(name, sid, record))
    }

    pub(super) fn enable(
        scheduler: &mut impl Scheduler,
        sid: &str,
        launch: &LaunchSpec,
        delay: u32,
    ) -> Result<StartupStatus, String> {
        // All local validation precedes even reading or replacing registration.
        let xml = definition(sid, launch, delay)?;
        let name = task_name(sid)?;
        let before = scheduler.read(&name)?;
        if before.as_ref().is_some_and(|r| !r.owned(sid)) {
            return Err("refusing to replace a foreign startup registration".into());
        }
        let canonical = scheduler.validate(&name, &xml)?;
        // Re-check after validation. The native operation lock serializes our
        // writers across sessions; a foreign writer is never intentionally won.
        if scheduler.read(&name)? != before {
            return Err(
                "startup registration changed while validating; no update attempted".into(),
            );
        }
        let outcome = scheduler
            .write(&name, &xml, before.is_none())
            .and_then(|()| {
                let result = status(scheduler, sid)?;
                if !result.registered
                    || !result.owned
                    || !result.enabled
                    || result.launch.as_ref() != Some(launch)
                    || result.delay_secs != Some(delay)
                {
                    return Err(
                        "startup write completed but verification differs; no action was run"
                            .into(),
                    );
                }
                Ok(result)
            });
        match outcome {
            Ok(result) => Ok(result),
            Err(failure) => Err(rollback(
                scheduler,
                sid,
                &name,
                before.as_ref(),
                &canonical,
                &failure,
            )),
        }
    }

    fn rollback(
        scheduler: &mut impl Scheduler,
        sid: &str,
        name: &str,
        previous: Option<&Record>,
        canonical: &str,
        failure: &str,
    ) -> String {
        let Some(previous) = previous else {
            return format!(
                "startup creation failed: {failure}; query startup status before retrying"
            );
        };
        match scheduler.read(name) {
            Ok(Some(current)) if &current == previous => format!("startup update failed; previous definition unchanged: {failure}"),
            Ok(Some(current)) if current.owned(sid) && current.xml == canonical => {
                match scheduler.write(name, &previous.xml, false) {
                    Ok(()) if scheduler.read(name).ok().flatten().as_ref() == Some(previous) => format!("startup update failed; previous definition restored and verified: {failure}"),
                    Ok(()) => format!("startup update failed: {failure}; rollback write completed but restoration could not be verified; query startup status"),
                    Err(error) => format!("startup update failed: {failure}; rollback failed: {error}; query startup status before retrying"),
                }
            }
            _ => format!("startup update failed: {failure}; rollback refused because registration is inaccessible, absent, or differs from the complete prepared definition"),
        }
    }

    pub(super) fn disable(
        scheduler: &mut impl Scheduler,
        sid: &str,
    ) -> Result<StartupStatus, String> {
        let name = task_name(sid)?;
        if let Some(before) = scheduler.read(&name)? {
            if !before.owned(sid) {
                return Err("refusing to delete a foreign startup registration".into());
            }
            if scheduler.read(&name)? != Some(before) {
                return Err("startup registration changed before deletion; nothing removed".into());
            }
            scheduler.delete(&name)?;
        }
        status(scheduler, sid)
    }

    fn xml_escape(value: &str) -> Result<String, String> {
        crate::text_valid(value)?;
        if value.chars().any(|c| {
            !(matches!(c, '\t' | '\n' | '\r')
                || ('\u{20}'..='\u{d7ff}').contains(&c)
                || ('\u{e000}'..='\u{fffd}').contains(&c)
                || c >= '\u{10000}')
        }) {
            return Err("value contains a character unsupported by Task Scheduler XML 1.0".into());
        }
        // Numeric whitespace references preserve argument fidelity through XML
        // newline normalization (including CR and tabs in quoted arguments).
        Ok(value
            .replace('&', "&amp;")
            .replace('<', "&lt;")
            .replace('>', "&gt;")
            .replace('"', "&quot;")
            .replace('\'', "&apos;")
            .replace('\r', "&#13;")
            .replace('\n', "&#10;")
            .replace('\t', "&#9;"))
    }

    pub(super) fn definition(sid: &str, spec: &LaunchSpec, delay: u32) -> Result<String, String> {
        validate_launch(spec)?;
        task_name(sid)?;
        if delay > 86400 {
            return Err("startup delay must be in 0..=86400 seconds".into());
        }
        // Task Scheduler expands %NAME% in Exec fields. Refuse those literals
        // instead of registering an action whose runtime argv could differ.
        let arguments = command_line::arguments(&spec.args);
        for text in [
            path_text(&spec.executable)?,
            path_text(&spec.working_directory)?,
            &arguments,
        ] {
            if text.matches('%').count() >= 2 {
                return Err("Task Scheduler environment expansion (%...%) is unsupported; supply literal absolute paths/arguments".into());
            }
        }
        let marker = xml_escape(&marker(sid))?;
        let exe = xml_escape(path_text(&spec.executable)?)?;
        let args = xml_escape(&arguments)?;
        let cwd = xml_escape(path_text(&spec.working_directory)?)?;
        let uri = xml_escape(&format!("\\{}", task_name(sid)?))?;
        Ok(format!(
            r#"<?xml version="1.0" encoding="UTF-16"?>
<Task version="1.2" xmlns="http://schemas.microsoft.com/windows/2004/02/mit/task">
 <RegistrationInfo><Description>{marker}</Description><Author>{sid}</Author><URI>{uri}</URI></RegistrationInfo>
 <Triggers><LogonTrigger><Enabled>true</Enabled><UserId>{sid}</UserId><Delay>PT{delay}S</Delay></LogonTrigger></Triggers>
 <Principals><Principal id="CurrentUser"><UserId>{sid}</UserId><LogonType>InteractiveToken</LogonType><RunLevel>LeastPrivilege</RunLevel></Principal></Principals>
 <Settings><MultipleInstancesPolicy>IgnoreNew</MultipleInstancesPolicy><DisallowStartIfOnBatteries>false</DisallowStartIfOnBatteries><StopIfGoingOnBatteries>false</StopIfGoingOnBatteries><AllowHardTerminate>false</AllowHardTerminate><StartWhenAvailable>false</StartWhenAvailable><RunOnlyIfNetworkAvailable>false</RunOnlyIfNetworkAvailable><AllowStartOnDemand>false</AllowStartOnDemand><Enabled>true</Enabled><Hidden>false</Hidden><WakeToRun>false</WakeToRun><ExecutionTimeLimit>PT0S</ExecutionTimeLimit></Settings>
 <Actions Context="CurrentUser"><Exec><Command>{exe}</Command><Arguments>{args}</Arguments><WorkingDirectory>{cwd}</WorkingDirectory></Exec></Actions>
</Task>"#
        ))
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        const SID: &str = "S-1-5-21-100-200-300-1001";
        #[derive(Default)]
        struct Fake {
            record: Option<Record>,
            next: Option<Record>,
            calls: Vec<String>,
            validate_fail: bool,
            write_fail: bool,
            rollback_fail: bool,
            read_fail: bool,
            previous: Option<Record>,
            corrupt_readback: bool,
        }
        impl Scheduler for Fake {
            fn read(&mut self, name: &str) -> Result<Option<Record>, String> {
                self.calls.push(format!("read:{name}"));
                if self.read_fail {
                    Err("access denied".into())
                } else {
                    Ok(self.record.clone())
                }
            }
            fn validate(&mut self, _: &str, xml: &str) -> Result<String, String> {
                self.calls.push("validate".into());
                self.previous = self.record.clone();
                if self.validate_fail {
                    Err("bad schema".into())
                } else {
                    Ok(xml.into())
                }
            }
            fn write(&mut self, _: &str, xml: &str, create: bool) -> Result<(), String> {
                self.calls.push(format!("write:{create}"));
                if self.rollback_fail
                    && self
                        .calls
                        .iter()
                        .filter(|c| c.starts_with("write:"))
                        .count()
                        > 1
                {
                    return Err("rollback denied".into());
                }
                if let Some(next) = self.next.take() {
                    self.record = Some(next);
                } else if self.previous.as_ref().is_some_and(|r| r.xml == xml) {
                    self.record = self.previous.clone();
                } else if let Some(r) = self.record.as_mut() {
                    r.xml = xml.into();
                }
                if self.corrupt_readback {
                    self.record.as_mut().unwrap().enabled = false;
                }
                if std::mem::take(&mut self.write_fail) {
                    Err("update failed after write".into())
                } else {
                    Ok(())
                }
            }
            fn delete(&mut self, _: &str) -> Result<(), String> {
                self.calls.push("delete".into());
                self.record = None;
                Ok(())
            }
        }
        fn record(spec: &LaunchSpec, delay: u32) -> Record {
            Record {
                xml: definition(SID, spec, delay).unwrap(),
                marker: marker(SID),
                principal_sid: SID.into(),
                group: String::new(),
                interactive: true,
                least_privilege: true,
                logon_sid: Some(SID.into()),
                enabled: true,
                launch: Some(spec.clone()),
                delay: Some(delay),
                warnings: vec![],
            }
        }
        #[test]
        fn status_is_read_only_absent_disabled_conflicting_and_inaccessible() {
            let mut fake = Fake::default();
            assert!(!status(&mut fake, SID).unwrap().registered);
            let mut r = record(&crate::tests::spec(), 6);
            r.enabled = false;
            fake.record = Some(r);
            let s = status(&mut fake, SID).unwrap();
            assert!(s.owned);
            assert!(!s.enabled);
            fake.record.as_mut().unwrap().principal_sid = "S-1-5-18".into();
            let s = status(&mut fake, SID).unwrap();
            assert!(s.registered);
            assert!(!s.owned);
            assert!(!s.warnings.is_empty());
            fake.read_fail = true;
            assert!(status(&mut fake, SID).unwrap_err().contains("not absent"));
            assert!(fake.calls.iter().all(|s| s.starts_with("read:")));
        }
        #[test]
        fn sid_principal_and_marker_are_required_for_mutations() {
            let spec = crate::tests::spec();
            for field in 0..5 {
                let mut r = record(&spec, 6);
                match field {
                    0 => r.marker = "TaskbarX".into(),
                    1 => r.principal_sid = "S-1-5-18".into(),
                    2 => r.least_privilege = false,
                    3 => r.interactive = false,
                    _ => r.logon_sid = None,
                }
                let mut f = Fake {
                    record: Some(r),
                    ..Default::default()
                };
                assert!(enable(&mut f, SID, &spec, 6).is_err());
                assert!(disable(&mut f, SID).is_err());
                assert!(f.calls.iter().all(|s| s.starts_with("read:")));
            }
        }
        #[test]
        fn delayed_logon_has_separate_fields_xml_escaping_and_no_implicit_run() {
            let mut spec = crate::tests::spec();
            spec.args.push("\u{96ea} & < > ' \" \\ \r\n\t".into());
            let xml = definition(SID, &spec, 86400).unwrap();
            for expected in [
                "PT86400S",
                "InteractiveToken",
                "LeastPrivilege",
                "<WorkingDirectory>",
                "&amp;",
                "&lt;",
                "&apos;",
                "&#13;",
                "DisallowStartIfOnBatteries>false",
                "StopIfGoingOnBatteries>false",
                "StartWhenAvailable>false",
                "AllowStartOnDemand>false",
                "ExecutionTimeLimit>PT0S",
            ] {
                assert!(xml.contains(expected), "{expected}");
            }
            assert!(!xml.contains("RegistrationTrigger"));
            // logonTriggerType extends triggerBaseType, then requires UserId
            // before Delay (unlike the xs:all used by Settings/RegistrationInfo).
            assert!(xml.contains(&format!(
                "<Enabled>true</Enabled><UserId>{SID}</UserId><Delay>PT86400S</Delay>"
            )));
            assert_eq!(
                task_name(SID).unwrap(),
                format!("TaskbarX-Rust.Startup.v1.{SID}")
            );
            let mut f = Fake {
                next: Some(record(&spec, 6)),
                ..Default::default()
            };
            assert!(enable(&mut f, SID, &spec, 6).unwrap().owned);
            assert!(enable(&mut f, SID, &spec, 6).unwrap().owned);
            assert_eq!(
                f.calls.iter().filter(|c| c.starts_with("write:")).count(),
                2
            );
            assert!(!f.calls.iter().any(|c| c.contains("run")));
            disable(&mut f, SID).unwrap();
            disable(&mut f, SID).unwrap();
            assert_eq!(f.calls.iter().filter(|c| *c == "delete").count(), 1);
        }
        #[test]
        fn invalid_input_and_validation_failure_cannot_replace_task() {
            let spec = crate::tests::spec();
            let mut f = Fake::default();
            assert!(enable(&mut f, SID, &spec, 86401).is_err());
            assert!(f.calls.is_empty());
            let mut bad = spec.clone();
            bad.args.push("bad\u{1}".into());
            assert!(enable(&mut f, SID, &bad, 0).is_err());
            assert!(f.calls.is_empty());
            bad.args = vec!["%APPDATA%".into()];
            assert!(enable(&mut f, SID, &bad, 0).is_err());
            bad.args = vec!["%APP".into(), "DATA%".into()];
            assert!(enable(&mut f, SID, &bad, 0).is_err());
            f.record = Some(record(&spec, 6));
            f.validate_fail = true;
            assert!(enable(&mut f, SID, &spec, 0).is_err());
            assert!(!f.calls.iter().any(|c| c.starts_with("write:")));
        }
        #[test]
        fn failed_update_restores_prior_definition_and_reports_failed_rollback() {
            let spec = crate::tests::spec();
            let old = record(&spec, 6);
            let mut f = Fake {
                record: Some(old.clone()),
                next: Some(record(&spec, 12)),
                write_fail: true,
                ..Default::default()
            };
            assert!(enable(&mut f, SID, &spec, 12)
                .unwrap_err()
                .contains("restored"));
            assert_eq!(f.record.unwrap(), old);
            let mut f = Fake {
                record: Some(old),
                next: Some(record(&spec, 12)),
                write_fail: true,
                rollback_fail: true,
                ..Default::default()
            };
            assert!(enable(&mut f, SID, &spec, 12)
                .unwrap_err()
                .contains("rollback failed"));
        }
        #[test]
        fn failed_readback_restores_but_never_overclaims_rollback_or_clobbers_changed_xml() {
            let spec = crate::tests::spec();
            let old = record(&spec, 6);
            let mut f = Fake {
                record: Some(old.clone()),
                next: Some(record(&spec, 12)),
                corrupt_readback: true,
                ..Default::default()
            };
            assert!(enable(&mut f, SID, &spec, 12)
                .unwrap_err()
                .contains("could not be verified"));
            let mut foreign = record(&spec, 12);
            foreign.xml.push_str("<!-- external edit -->");
            let mut f = Fake {
                record: Some(old),
                next: Some(foreign.clone()),
                write_fail: true,
                ..Default::default()
            };
            assert!(enable(&mut f, SID, &spec, 12)
                .unwrap_err()
                .contains("rollback refused"));
            assert_eq!(f.record, Some(foreign));
            assert_eq!(
                f.calls.iter().filter(|c| c.starts_with("write:")).count(),
                1
            );
        }
    }
}
