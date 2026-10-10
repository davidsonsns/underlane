//! Platform integration: launch at login, App Nap, reading the real status item title.

use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct LoginItem {
    /// "enabled" | "not_registered" | "requires_approval" | "not_found" | "unsupported"
    pub status: String,
    pub enabled: bool,
    pub message: Option<String>,
}

#[cfg(target_os = "macos")]
mod imp {
    use super::LoginItem;
    use objc2::rc::Retained;
    use objc2::runtime::ProtocolObject;
    use objc2_foundation::{NSActivityOptions, NSObjectProtocol, NSProcessInfo, NSString};
    use objc2_service_management::{SMAppService, SMAppServiceStatus};

    fn describe(status: SMAppServiceStatus) -> (&'static str, bool) {
        match status {
            SMAppServiceStatus::Enabled => ("enabled", true),
            SMAppServiceStatus::RequiresApproval => ("requires_approval", false),
            SMAppServiceStatus::NotFound => ("not_found", false),
            _ => ("not_registered", false),
        }
    }

    pub fn login_item() -> LoginItem {
        let status = unsafe { SMAppService::mainAppService().status() };
        let (s, enabled) = describe(status);
        LoginItem {
            status: s.into(),
            enabled,
            message: (s == "requires_approval")
                .then(|| "Allow Burrow in System Settings → General → Login Items.".to_string()),
        }
    }

    pub fn set_login_item(enabled: bool) -> LoginItem {
        let svc = unsafe { SMAppService::mainAppService() };
        let result = unsafe {
            if enabled {
                svc.registerAndReturnError()
            } else {
                svc.unregisterAndReturnError()
            }
        };
        let mut item = login_item();
        if let Err(e) = result {
            item.message = Some(format!("macOS refused the change: {}", e.localizedDescription()));
        }
        item
    }

    /// Holds an `NSProcessInfo` activity; ends it on drop.
    pub struct Activity(Retained<ProtocolObject<dyn NSObjectProtocol>>);
    // NSProcessInfo activity tokens may be ended from any thread.
    unsafe impl Send for Activity {}
    unsafe impl Sync for Activity {}

    impl Activity {
        pub fn begin(reason: &str) -> Option<Activity> {
            let info = NSProcessInfo::processInfo();
            let token = info.beginActivityWithOptions_reason(
                NSActivityOptions::UserInitiatedAllowingIdleSystemSleep,
                &NSString::from_str(reason),
            );
            Some(Activity(token))
        }
    }

    impl Drop for Activity {
        fn drop(&mut self) {
            unsafe { NSProcessInfo::processInfo().endActivity(&self.0) };
        }
    }
}

#[cfg(not(target_os = "macos"))]
mod imp {
    use super::LoginItem;

    pub fn login_item() -> LoginItem {
        LoginItem { status: "unsupported".into(), enabled: false, message: Some("Launch at login uses the autostart plugin on this platform.".into()) }
    }

    pub fn set_login_item(_enabled: bool) -> LoginItem {
        login_item()
    }

    pub struct Activity;
    impl Activity {
        pub fn begin(_reason: &str) -> Option<Activity> {
            None
        }
    }
}

pub use imp::{login_item, set_login_item, Activity};
