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

    pub fn set_enabled(enabled: bool) {
        let Some(service) = main_app() else {
            return;
        };
        let error: *mut *mut AnyObject = std::ptr::null_mut();
        let _: Bool = if enabled {
            unsafe { msg_send![&service, registerAndReturnError: error] }
        } else {
            unsafe { msg_send![&service, unregisterAndReturnError: error] }
        };
    }
}

#[cfg(not(target_os = "macos"))]
mod imp {
    pub fn is_enabled() -> bool {
        false
    }

    pub fn set_enabled(_enabled: bool) {}
}

pub use imp::{is_enabled, set_enabled};
