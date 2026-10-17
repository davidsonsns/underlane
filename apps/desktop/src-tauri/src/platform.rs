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
    use objc2_foundation::{NSActivityOptions, NSObjectProtocol, NSProcessInfo, NSString, NSUserDefaults};
    use objc2_service_management::{SMAppService, SMAppServiceStatus};

    /// True when macOS opened the app as a login item: the launch Apple event carries
    /// `keyAEPropData = keyAELaunchedAsLogInItem` ('lgit'). Call during did-finish-launching.
    pub fn launched_as_login_item() -> bool {
        const KEY_AE_PROP_DATA: u32 = u32::from_be_bytes(*b"prdt");
        const LAUNCHED_AS_LOGIN_ITEM: u32 = u32::from_be_bytes(*b"lgit");
        let mgr = objc2_foundation::NSAppleEventManager::sharedAppleEventManager();
        let Some(event) = mgr.currentAppleEvent() else { return false };
        match event.paramDescriptorForKeyword(KEY_AE_PROP_DATA) {
            Some(d) => d.enumCodeValue() == LAUNCHED_AS_LOGIN_ITEM || d.typeCodeValue() == LAUNCHED_AS_LOGIN_ITEM,
            None => false,
        }
    }

    /// A menu bar app has nothing to restore. Without this, a crash or force quit makes macOS
    /// show a modal "reopen windows?" prompt at the next launch, which blocks startup.
    pub fn disable_state_restoration() {
        let defaults = NSUserDefaults::standardUserDefaults();
        defaults.setBool_forKey(true, &NSString::from_str("ApplePersistenceIgnoreState"));
        defaults.setBool_forKey(false, &NSString::from_str("NSQuitAlwaysKeepsWindows"));
        if let Some(home) = std::env::var_os("HOME") {
            let saved = std::path::Path::new(&home).join("Library/Saved Application State/app.underlane.savedState");
            let _ = std::fs::remove_dir_all(saved);
        }
    }

    /// A dev build runs unbundled, so the Dock shows a generic "exec" tile. Tauri sets the icon
    /// once at startup, but macOS drops it when the activation policy turns Regular; call after that.
    pub fn set_dock_icon() {
        #[cfg(debug_assertions)]
        {
            use objc2::AllocAnyThread;
            use objc2_app_kit::{NSApplication, NSImage};
            let Some(mtm) = objc2::MainThreadMarker::new() else { return };
            let data = objc2_foundation::NSData::with_bytes(include_bytes!("../icons/icon.png"));
            if let Some(icon) = NSImage::initWithData(NSImage::alloc(), &data) {
                unsafe { NSApplication::sharedApplication(mtm).setApplicationIconImage(Some(&icon)) };
            }
        }
    }

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
                .then(|| "Allow Underlane in System Settings → General → Login Items.".to_string()),
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

    pub fn disable_state_restoration() {}

    pub fn set_dock_icon() {}

    pub fn launched_as_login_item() -> bool {
        false
    }

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

pub use imp::{disable_state_restoration, launched_as_login_item, login_item, set_dock_icon, set_login_item, Activity};
