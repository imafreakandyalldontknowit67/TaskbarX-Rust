//! Shared native lifetime guards. None of these constructors is enabled in
//! unit-test builds, even if a test accidentally calls a public native API.
use std::{marker::PhantomData, rc::Rc};
use windows::{
    core::{PCWSTR, PWSTR},
    Win32::{
        Foundation::{
            CloseHandle, LocalFree, HANDLE, HLOCAL, RPC_E_CHANGED_MODE, WAIT_ABANDONED,
            WAIT_OBJECT_0,
        },
        Security::{
            Authorization::ConvertSidToStringSidW, GetTokenInformation, TokenUser, TOKEN_QUERY,
            TOKEN_USER,
        },
        System::{
            Com::{CoInitializeEx, CoUninitialize, COINIT_APARTMENTTHREADED},
            Threading::{
                CreateMutexW, GetCurrentProcess, OpenProcessToken, ReleaseMutex,
                WaitForSingleObject,
            },
        },
    },
};

pub(crate) fn effects_allowed() -> Result<(), String> {
    if cfg!(test) {
        Err("native shell effects are disabled in unit tests".into())
    } else {
        Ok(())
    }
}

pub(crate) fn wide(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(Some(0)).collect()
}

pub(crate) fn decode(value: &[u16]) -> Result<String, String> {
    String::from_utf16(value)
        .map_err(|_| "native shell value has unsupported UTF-16 encoding".into())
}

pub(crate) struct Apartment {
    uninitialize: bool,
    _thread: PhantomData<Rc<()>>,
}
impl Apartment {
    pub(crate) fn new() -> Result<Self, String> {
        effects_allowed()?;
        let hr = unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) };
        // A caller's existing MTA is usable; do not uninitialize its apartment.
        if hr != RPC_E_CHANGED_MODE {
            hr.ok().map_err(|e| format!("initializing COM: {e}"))?;
        }
        Ok(Self {
            uninitialize: hr.is_ok(),
            _thread: PhantomData,
        })
    }
}
impl Drop for Apartment {
    fn drop(&mut self) {
        if self.uninitialize {
            unsafe {
                CoUninitialize();
            }
        }
    }
}

struct Handle(HANDLE);
impl Drop for Handle {
    fn drop(&mut self) {
        unsafe {
            let _ = CloseHandle(self.0);
        }
    }
}

pub(crate) fn user_sid() -> Result<String, String> {
    effects_allowed()?;
    unsafe {
        let mut raw = HANDLE::default();
        OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut raw)
            .map_err(|e| format!("opening current-user token: {e}"))?;
        let token = Handle(raw);
        let mut size = 0;
        let _ = GetTokenInformation(token.0, TokenUser, None, 0, &mut size);
        if size == 0 || size > 65536 {
            return Err("invalid token information size".into());
        }
        let mut data = vec![0usize; (size as usize).div_ceil(std::mem::size_of::<usize>())];
        GetTokenInformation(
            token.0,
            TokenUser,
            Some(data.as_mut_ptr().cast()),
            size,
            &mut size,
        )
        .map_err(|e| format!("reading current-user token: {e}"))?;
        let user = &*data.as_ptr().cast::<TOKEN_USER>();
        let mut string = PWSTR::null();
        ConvertSidToStringSidW(user.User.Sid, &mut string)
            .map_err(|e| format!("converting current-user SID: {e}"))?;
        // SID strings have a documented, bounded grammar. LocalFree is required
        // even if the unexpected encoding check fails.
        let result = decode(string.as_wide());
        let _ = LocalFree(Some(HLOCAL(string.0.cast())));
        result
    }
}

pub(crate) struct OperationLock {
    handle: Handle,
    _thread: PhantomData<Rc<()>>,
}
impl OperationLock {
    pub(crate) fn acquire(key: &str) -> Result<Self, String> {
        effects_allowed()?;
        let name = wide(&format!("Global\\TaskbarX.Rust.Shell.v1.{key}"));
        let handle = Handle(
            unsafe { CreateMutexW(None, false, PCWSTR(name.as_ptr())) }
                .map_err(|e| format!("creating shell-operation mutex: {e}"))?,
        );
        match unsafe { WaitForSingleObject(handle.0, 5000) } {
            WAIT_OBJECT_0 | WAIT_ABANDONED => Ok(Self {
                handle,
                _thread: PhantomData,
            }),
            _ => Err("could not acquire shell-operation mutex within five seconds".into()),
        }
    }
}
impl Drop for OperationLock {
    fn drop(&mut self) {
        unsafe {
            let _ = ReleaseMutex(self.handle.0);
        }
    }
}
