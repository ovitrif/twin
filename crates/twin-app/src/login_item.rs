//! "Open at Login" through macOS's ServiceManagement (`SMAppService`, macOS
//! 13+): registering the app adds it under System Settings → General → Login
//! Items. Reading and changing it needs no permission prompt, unlike the
//! System Events login items Twin used before.

#[cfg(target_os = "macos")]
mod imp {
    use objc2::msg_send;
    use objc2::rc::Retained;
    use objc2::runtime::{AnyClass, AnyObject, Bool};

    #[link(name = "ServiceManagement", kind = "framework")]
    unsafe extern "C" {}

    /// `SMAppServiceStatusEnabled`
    const ENABLED: isize = 1;

    /// The service for this app's own bundle, or none outside a bundle (a
    /// dev build), where there is nothing to register.
    fn main_app() -> Option<Retained<AnyObject>> {
        let exe = std::env::current_exe().ok()?;
        if !exe.to_string_lossy().contains(".app/Contents/MacOS/") {
            return None;
        }
        let class = AnyClass::get(c"SMAppService")?;
        unsafe { msg_send![class, mainAppService] }
    }

    pub fn is_enabled() -> bool {
        main_app().is_some_and(|service| {
            let status: isize = unsafe { msg_send![&service, status] };
            status == ENABLED
        })
    }

    /// Registers or unregisters the app; the error says why macOS refused
    pub fn set_enabled(enabled: bool) -> Result<(), String> {
        let Some(service) = main_app() else {
            return Err("only an installed Twin.app can open at login".to_string());
        };
        objc2::rc::autoreleasepool(|_| {
            let mut error: *mut AnyObject = std::ptr::null_mut();
            let error_out: *mut *mut AnyObject = &mut error;
            let done: Bool = if enabled {
                unsafe { msg_send![&service, registerAndReturnError: error_out] }
            } else {
                unsafe { msg_send![&service, unregisterAndReturnError: error_out] }
            };
            if done.as_bool() {
                Ok(())
            } else {
                Err(describe(error))
            }
        })
    }

    /// An `NSError`'s localized description
    fn describe(error: *mut AnyObject) -> String {
        if error.is_null() {
            return "unknown error".to_string();
        }
        unsafe {
            let text: *mut AnyObject = msg_send![error, localizedDescription];
            if text.is_null() {
                return "unknown error".to_string();
            }
            let utf8: *const std::ffi::c_char = msg_send![text, UTF8String];
            if utf8.is_null() {
                return "unknown error".to_string();
            }
            std::ffi::CStr::from_ptr(utf8)
                .to_string_lossy()
                .into_owned()
        }
    }
}

#[cfg(not(target_os = "macos"))]
mod imp {
    pub fn is_enabled() -> bool {
        false
    }

    pub fn set_enabled(_enabled: bool) -> Result<(), String> {
        Err("opening at login needs macOS".to_string())
    }
}

pub use imp::{is_enabled, set_enabled};
