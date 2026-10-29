use super::protocol::{Record, Scheduler};
use crate::{
    command_line,
    native::{decode, Apartment},
    LaunchSpec,
};
use windows::{
    core::{Interface, BSTR},
    Win32::{
        Foundation::{ERROR_FILE_NOT_FOUND, VARIANT_BOOL},
        System::{
            Com::{CoCreateInstance, CLSCTX_INPROC_SERVER},
            TaskScheduler::{
                IExecAction, ILogonTrigger, IRegisteredTask, ITaskDefinition, ITaskFolder,
                ITaskService, TaskScheduler, TASK_CREATE, TASK_IGNORE_REGISTRATION_TRIGGERS,
                TASK_LOGON_INTERACTIVE_TOKEN, TASK_LOGON_TYPE, TASK_RUNLEVEL_LUA,
                TASK_RUNLEVEL_TYPE, TASK_UPDATE, TASK_VALIDATE_ONLY,
            },
            Variant::VARIANT,
        },
    },
};

pub(super) struct SchedulerCom {
    folder: ITaskFolder,
    service: ITaskService,
    // Release interfaces before balancing CoInitializeEx.
    _apartment: Apartment,
}

fn string(get: impl FnOnce(*mut BSTR) -> windows::core::Result<()>) -> Result<String, String> {
    let mut value = BSTR::new();
    get(&mut value).map_err(|e| format!("reading task property: {e}"))?;
    decode(&value)
}

impl SchedulerCom {
    pub(super) fn connect() -> Result<Self, String> {
        let apartment = Apartment::new()?;
        unsafe {
            let service: ITaskService =
                CoCreateInstance(&TaskScheduler, None, CLSCTX_INPROC_SERVER)
                    .map_err(|e| format!("opening Task Scheduler COM: {e}"))?;
            let empty = VARIANT::default();
            service
                .Connect(&empty, &empty, &empty, &empty)
                .map_err(|e| format!("connecting to local Task Scheduler (no credentials): {e}"))?;
            // Query the existing root. Never CreateFolder, including on absence.
            let folder = service
                .GetFolder(&BSTR::from("\\"))
                .map_err(|e| format!("opening existing scheduler root: {e}"))?;
            Ok(Self {
                folder,
                service,
                _apartment: apartment,
            })
        }
    }

    fn definition(&self, xml: &str) -> Result<ITaskDefinition, String> {
        unsafe {
            let definition = self
                .service
                .NewTask(0)
                .map_err(|e| format!("allocating task definition: {e}"))?;
            definition
                .SetXmlText(&BSTR::from(xml))
                .map_err(|e| format!("validating task XML: {e}"))?;
            Ok(definition)
        }
    }

    fn register(&self, name: &str, xml: &str, flags: i32) -> Result<(), String> {
        let definition = self.definition(xml)?;
        let name = BSTR::from(name);
        unsafe {
            let mut result = std::ptr::null_mut();
            // The generated wrapper expects a non-null IRegisteredTask even
            // for TASK_VALIDATE_ONLY, whose successful result may be null.
            // Calling the same vtable slot explicitly preserves that success.
            let hr = (self.folder.vtable().RegisterTaskDefinition)(
                self.folder.as_raw(),
                name.as_ptr().cast_mut().cast(),
                definition.as_raw(),
                flags,
                VARIANT::default(),
                VARIANT::default(),
                TASK_LOGON_INTERACTIVE_TOKEN,
                VARIANT::default(),
                &mut result,
            );
            if !result.is_null() {
                drop(IRegisteredTask::from_raw(result));
            }
            hr.ok()
                .map_err(|e| format!("Task Scheduler registration/validation failed: {e}"))?;
            // Positive HRESULT warnings (including SOME_TRIGGERS_FAILED) are
            // not proof of a working logon registration. Surface them so the
            // transaction can restore the previous owned definition.
            if hr.0 != 0 {
                return Err(format!("Task Scheduler returned non-success warning HRESULT 0x{:08x}; registration is not confirmed", hr.0 as u32));
            }
            Ok(())
        }
    }
}

impl Scheduler for SchedulerCom {
    fn read(&mut self, name: &str) -> Result<Option<Record>, String> {
        unsafe {
            let task = match self.folder.GetTask(&BSTR::from(name)) {
                Ok(task) => task,
                Err(e) if e.code() == ERROR_FILE_NOT_FOUND.to_hresult() => return Ok(None),
                Err(e) => return Err(format!("reading startup task (state unknown): {e}")),
            };
            let definition = task
                .Definition()
                .map_err(|e| format!("reading task definition: {e}"))?;
            let info = definition.RegistrationInfo().map_err(|e| e.to_string())?;
            let principal = definition.Principal().map_err(|e| e.to_string())?;
            let mut logon = TASK_LOGON_TYPE::default();
            let mut level = TASK_RUNLEVEL_TYPE::default();
            principal.LogonType(&mut logon).map_err(|e| e.to_string())?;
            principal.RunLevel(&mut level).map_err(|e| e.to_string())?;
            let mut record = Record {
                xml: string(|out| definition.XmlText(out))?,
                marker: string(|out| info.Description(out))?,
                principal_sid: normalize_account(
                    &string(|out| principal.UserId(out))?,
                    account_sid,
                )?,
                group: string(|out| principal.GroupId(out))?,
                interactive: logon == TASK_LOGON_INTERACTIVE_TOKEN,
                least_privilege: level == TASK_RUNLEVEL_LUA,
                logon_sid: None,
                enabled: task.Enabled().map_err(|e| e.to_string())?.as_bool(),
                launch: None,
                delay: None,
                warnings: vec![],
            };
            let triggers = definition.Triggers().map_err(|e| e.to_string())?;
            let mut count = 0;
            triggers.Count(&mut count).map_err(|e| e.to_string())?;
            if count == 1 {
                let trigger = triggers.get_Item(1).map_err(|e| e.to_string())?;
                if let Ok(logon) = trigger.cast::<ILogonTrigger>() {
                    record.logon_sid = Some(normalize_account(
                        &string(|out| logon.UserId(out))?,
                        account_sid,
                    )?);
                    let delay = string(|out| logon.Delay(out))?;
                    record.delay = parse_duration(&delay);
                    if record.delay.is_none() {
                        record
                            .warnings
                            .push(format!("unsupported stored logon delay: {delay}"));
                    }
                    let mut enabled = VARIANT_BOOL::default();
                    trigger.Enabled(&mut enabled).map_err(|e| e.to_string())?;
                    if !enabled.as_bool() {
                        record.enabled = false;
                        record.warnings.push(
                            "logon trigger is disabled (effective startup is disabled)".into(),
                        );
                    }
                }
            }
            if record.logon_sid.is_none() {
                record
                    .warnings
                    .push("expected exactly one current-user logon trigger".into());
            }
            let actions = definition.Actions().map_err(|e| e.to_string())?;
            actions.Count(&mut count).map_err(|e| e.to_string())?;
            if count == 1 {
                if let Ok(exec) = actions
                    .get_Item(1)
                    .map_err(|e| e.to_string())?
                    .cast::<IExecAction>()
                {
                    let executable = string(|out| exec.Path(out))?;
                    let args = string(|out| exec.Arguments(out))?;
                    let cwd = string(|out| exec.WorkingDirectory(out))?;
                    match command_line::parse(&args) {
                        Ok(args) => {
                            let launch = LaunchSpec {
                                executable: executable.into(),
                                args,
                                working_directory: cwd.into(),
                            };
                            match crate::validate_launch(&launch) {
                                Ok(()) => record.launch = Some(launch),
                                Err(e) => record
                                    .warnings
                                    .push(format!("stored launch is invalid: {e}")),
                            }
                        }
                        Err(e) => record.warnings.push(e),
                    }
                }
            }
            if record.launch.is_none() {
                record
                    .warnings
                    .push("expected one readable absolute executable action".into());
            }
            Ok(Some(record))
        }
    }

    fn validate(&mut self, name: &str, xml: &str) -> Result<String, String> {
        self.register(name, xml, TASK_VALIDATE_ONLY.0)?;
        let definition = self.definition(xml)?;
        // COM's normalized serialization is used for rollback identity checks;
        // comparing only action/principal could clobber unrelated settings.
        unsafe { string(|out| definition.XmlText(out)) }
    }

    fn write(&mut self, name: &str, xml: &str, create: bool) -> Result<(), String> {
        let mode = if create { TASK_CREATE } else { TASK_UPDATE };
        // Do not use CREATE_OR_UPDATE: it would overwrite an absent->foreign
        // collision. Never Run(), RunEx(), or allow registration triggers.
        self.register(name, xml, mode.0 | TASK_IGNORE_REGISTRATION_TRIGGERS.0)
    }

    fn delete(&mut self, name: &str) -> Result<(), String> {
        match unsafe { self.folder.DeleteTask(&BSTR::from(name), 0) } {
            Ok(()) => Ok(()),
            Err(e) if e.code() == ERROR_FILE_NOT_FOUND.to_hresult() => Ok(()),
            Err(e) => Err(format!("removing owned startup registration: {e}")),
        }
    }
}

// Scheduler may return a bare or DOMAIN\\user account name even when the
// registered XML used a SID. Resolve names, never compare display names or
// weaken the marker/principal/logon ownership checks.
fn normalize_account(
    value: &str,
    resolve: impl FnOnce(&str) -> Result<String, String>,
) -> Result<String, String> {
    if value.is_empty() || value.starts_with("S-1-") {
        Ok(value.to_owned())
    } else {
        resolve(value)
    }
}

fn account_sid(name: &str) -> Result<String, String> {
    use windows::{
        core::{PCWSTR, PWSTR},
        Win32::{
            Foundation::{LocalFree, ERROR_INSUFFICIENT_BUFFER, HLOCAL},
            Security::{
                Authorization::ConvertSidToStringSidW, LookupAccountNameW, PSID, SID_NAME_USE,
            },
        },
    };
    crate::native::effects_allowed()?;
    let name = crate::native::wide(name);
    let mut bytes = 0;
    let mut domain_len = 0;
    let mut kind = SID_NAME_USE::default();
    unsafe {
        match LookupAccountNameW(
            PCWSTR::null(),
            PCWSTR(name.as_ptr()),
            None,
            &mut bytes,
            None,
            &mut domain_len,
            &mut kind,
        ) {
            Err(e) if e.code() == ERROR_INSUFFICIENT_BUFFER.to_hresult() => {}
            Err(e) => return Err(format!("resolving scheduled-task account identity: {e}")),
            Ok(()) => return Err("account lookup returned no SID buffer".into()),
        }
        if bytes == 0 || bytes > 65536 || domain_len > 65536 {
            return Err("invalid scheduled-task account buffer sizes".into());
        }
        // Aligned storage for the native SID; lookup lengths remain bytes.
        let mut sid = vec![0u32; (bytes as usize).div_ceil(4)];
        let mut domain = vec![0u16; domain_len as usize];
        LookupAccountNameW(
            PCWSTR::null(),
            PCWSTR(name.as_ptr()),
            Some(PSID(sid.as_mut_ptr().cast())),
            &mut bytes,
            Some(PWSTR(domain.as_mut_ptr())),
            &mut domain_len,
            &mut kind,
        )
        .map_err(|e| format!("resolving scheduled-task account identity: {e}"))?;
        let mut text = PWSTR::null();
        ConvertSidToStringSidW(PSID(sid.as_mut_ptr().cast()), &mut text)
            .map_err(|e| format!("formatting scheduled-task account SID: {e}"))?;
        let result = text
            .to_string()
            .map_err(|e| format!("decoding scheduled-task account SID: {e}"));
        let _ = LocalFree(Some(HLOCAL(text.0.cast())));
        result
    }
}

fn parse_duration(text: &str) -> Option<u32> {
    if text.is_empty() {
        return Some(0);
    }
    // Scheduler can normalize PT86400S to P1D or PT24H. Accept integral
    // day/hour/minute/second components, no months, years or fractions.
    let text = text.strip_prefix('P')?;
    let mut total = 0u32;
    let mut digits = String::new();
    let mut time = false;
    let mut last_order = 0;
    let mut components = 0;
    let mut after_t = false;
    for c in text.chars() {
        if c.is_ascii_digit() {
            digits.push(c);
            continue;
        }
        if c == 'T' && digits.is_empty() && !time {
            time = true;
            after_t = true;
            continue;
        }
        let (factor, order) = match c {
            'D' if !time => (86400, 1),
            'H' if time => (3600, 2),
            'M' if time => (60, 3),
            'S' if time => (1, 4),
            _ => return None,
        };
        if order <= last_order {
            return None;
        }
        last_order = order;
        components += 1;
        after_t = false;
        total = total.checked_add(digits.parse::<u32>().ok()?.checked_mul(factor)?)?;
        digits.clear();
    }
    (digits.is_empty() && components > 0 && !after_t).then_some(total)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn scheduler_account_names_resolve_to_sids_without_trusting_names() {
        let sid = "S-1-5-21-100-200-300-1001";
        for name in ["patter", "DESKTOP\\patter"] {
            assert_eq!(normalize_account(name, |_| Ok(sid.into())).unwrap(), sid);
        }
        assert_eq!(
            normalize_account(sid, |_| panic!("SID must not be resolved as a name")).unwrap(),
            sid
        );
        assert_eq!(
            normalize_account("", |_| panic!(
                "empty trigger must not resolve to current user"
            ))
            .unwrap(),
            ""
        );
        assert_ne!(
            normalize_account("other", |_| Ok("S-1-5-21-999-1002".into())).unwrap(),
            sid
        );
        assert!(normalize_account("unresolved", |_| Err("lookup failed".into())).is_err());
    }
    #[test]
    fn scheduler_duration_normalization() {
        for (input, value) in [
            ("PT6S", 6),
            ("P1D", 86400),
            ("PT24H", 86400),
            ("PT1H2M3S", 3723),
            ("", 0),
        ] {
            assert_eq!(parse_duration(input), Some(value));
        }
        for input in [
            "P",
            "PT",
            "P1DT",
            "PT1S1M",
            "PT1S2S",
            "PT0.5S",
            "P1M",
            "PT99999999999999S",
            "PT-1S",
        ] {
            assert_eq!(parse_duration(input), None);
        }
    }
}
