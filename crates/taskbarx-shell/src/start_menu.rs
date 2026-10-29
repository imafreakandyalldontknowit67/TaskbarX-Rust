/// Send the classic shell's Start-menu system command. Success confirms message
/// delivery, not visual state. No engine, process, keyboard injection, taskbar
/// repositioning, or hidden-control mutation is performed.
pub fn show_start_menu() -> Result<(), String> {
    #[cfg(windows)]
    {
        request(&mut NativeStart)
    }
    #[cfg(not(windows))]
    Err(crate::unsupported())
}

#[cfg(any(windows, test))]
trait StartMenu {
    fn activate(&mut self) -> Result<(), String>;
}
#[cfg(any(windows, test))]
fn request(adapter: &mut impl StartMenu) -> Result<(), String> {
    adapter.activate()
}

#[cfg(windows)]
struct NativeStart;
#[cfg(windows)]
impl StartMenu for NativeStart {
    fn activate(&mut self) -> Result<(), String> {
        use windows::{
            core::w,
            Win32::{
                Foundation::{GetLastError, LPARAM, WPARAM},
                UI::WindowsAndMessaging::{
                    FindWindowExW, FindWindowW, SendMessageTimeoutW, SC_TASKLIST, SMTO_ABORTIFHUNG,
                    SMTO_ERRORONEXIT, WM_SYSCOMMAND,
                },
            },
        };
        crate::native::effects_allowed()?;
        unsafe {
            let shell = FindWindowW(w!("Shell_TrayWnd"), None)
                .map_err(|e| format!("classic shell taskbar unavailable: {e}"))?;
            if FindWindowExW(
                Some(shell),
                None,
                w!("Windows.UI.Composition.DesktopWindowContentBridge"),
                None,
            )
            .is_ok()
            {
                return Err("native modern Windows 11 taskbar is unsupported".into());
            }
            // Confirm classic controls rather than applying a compatibility
            // workaround or changing a hidden Start button.
            FindWindowExW(Some(shell), None, w!("ReBarWindow32"), None).map_err(|_| {
                "classic taskbar controls unavailable; Start action not attempted".to_string()
            })?;
            let mut result = 0;
            if SendMessageTimeoutW(
                shell,
                WM_SYSCOMMAND,
                WPARAM(SC_TASKLIST as usize),
                LPARAM(0),
                SMTO_ABORTIFHUNG | SMTO_ERRORONEXIT,
                2000,
                Some(&mut result),
            )
            .0 == 0
            {
                return Err(format!(
                    "Start-menu command delivery failed or timed out: {}",
                    GetLastError().to_hresult()
                ));
            }
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn start_menu_uses_only_injected_adapter_and_propagates_failure() {
        struct Fake {
            calls: usize,
            fail: bool,
        }
        impl StartMenu for Fake {
            fn activate(&mut self) -> Result<(), String> {
                self.calls += 1;
                if self.fail {
                    Err("mock unavailable".into())
                } else {
                    Ok(())
                }
            }
        }
        let mut f = Fake {
            calls: 0,
            fail: false,
        };
        request(&mut f).unwrap();
        f.fail = true;
        assert!(request(&mut f).unwrap_err().contains("mock"));
        assert_eq!(f.calls, 2);
    }
}
