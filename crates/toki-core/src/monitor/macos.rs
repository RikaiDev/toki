use anyhow::Result;
use async_trait::async_trait;
use chrono::Utc;
use cocoa::base::{id, nil};
use cocoa::foundation::NSAutoreleasePool;
use objc::{class, msg_send, sel, sel_impl};

use super::{AppActivity, SystemMonitor};

use tokio::process::Command;

// CoreGraphics bindings for idle time detection
#[link(name = "CoreGraphics", kind = "framework")]
extern "C" {
    fn CGEventSourceSecondsSinceLastEventType(
        source_state_id: u32,
        event_type: u32,
    ) -> f64;
}

// CGEventSourceStateID
const K_CG_EVENT_SOURCE_STATE_COMBINED_SESSION_STATE: u32 = 0;

// CGEventType - we check for any HID (Human Interface Device) event
const K_CG_ANY_INPUT_EVENT_TYPE: u32 = u32::MAX; // kCGAnyInputEventType

pub struct MacOSMonitor;

impl MacOSMonitor {
    /// Create a new macOS monitor
    ///
    /// # Errors
    ///
    /// Currently always succeeds, but returns `Result` for consistency with other platforms
    pub fn new() -> Result<Self> {
        Ok(Self)
    }

    /// Get system idle time in seconds using CoreGraphics API
    /// This returns the time since the last keyboard/mouse/trackpad event
    fn get_system_idle_seconds() -> f64 {
        unsafe {
            CGEventSourceSecondsSinceLastEventType(
                K_CG_EVENT_SOURCE_STATE_COMBINED_SESSION_STATE,
                K_CG_ANY_INPUT_EVENT_TYPE,
            )
        }
    }

    fn get_frontmost_app() -> Option<AppActivity> {
        unsafe {
            let _pool = NSAutoreleasePool::new(nil);

            // Get current app
            let workspace: id = msg_send![class!(NSWorkspace), sharedWorkspace];
            let frontmost_app: id = msg_send![workspace, frontmostApplication];

            if frontmost_app == nil {
                return None;
            }

            // Get bundle identifier
            let bundle_id: id = msg_send![frontmost_app, bundleIdentifier];
            let bundle_id_str = if bundle_id.is_null() {
                String::from("unknown")
            } else {
                let bytes: *const u8 = msg_send![bundle_id, UTF8String];
                let len: usize = msg_send![bundle_id, length];
                let slice = std::slice::from_raw_parts(bytes, len);
                String::from_utf8_lossy(slice).to_string()
            };

            // Get localized name
            let app_name: id = msg_send![frontmost_app, localizedName];
            let app_name_str = if app_name.is_null() {
                String::from("Unknown")
            } else {
                let bytes: *const u8 = msg_send![app_name, UTF8String];
                let len: usize = msg_send![app_name, length];
                let slice = std::slice::from_raw_parts(bytes, len);
                String::from_utf8_lossy(slice).to_string()
            };

            Some(AppActivity {
                app_id: bundle_id_str,
                app_name: app_name_str,
                window_title: None,
                is_active: true,
                timestamp: Utc::now(),
            })
        }
    }
}

#[async_trait]
impl SystemMonitor for MacOSMonitor {
    async fn start_monitoring(&mut self) -> Result<()> {
        log::info!("Started macOS activity monitoring");
        Ok(())
    }

    async fn get_active_app(&self) -> Result<Option<AppActivity>> {
        // Use AppleScript to get BOTH bundle ID and window title consistently
        // This ensures they refer to the same frontmost app
        let script = r#"
            tell application "System Events"
                set frontProc to first application process whose frontmost is true
                set bundleId to bundle identifier of frontProc
                set appName to name of frontProc
                try
                    set winTitle to name of first window of frontProc
                on error
                    set winTitle to ""
                end try
                return bundleId & "|" & appName & "|" & winTitle
            end tell
        "#;

        let output = Command::new("osascript")
            .arg("-e")
            .arg(script)
            .output()
            .await;

        if let Ok(output) = output {
            if output.status.success() {
                let result = String::from_utf8_lossy(&output.stdout).trim().to_string();
                let parts: Vec<&str> = result.splitn(3, '|').collect();

                if parts.len() >= 2 {
                    let bundle_id = parts[0].to_string();
                    let app_name = parts[1].to_string();
                    let window_title = if parts.len() > 2 && !parts[2].is_empty() {
                        Some(parts[2].to_string())
                    } else {
                        None
                    };

                    return Ok(Some(AppActivity {
                        app_id: bundle_id,
                        app_name,
                        window_title,
                        is_active: true,
                        timestamp: Utc::now(),
                    }));
                }
            }
        }

        // Fallback to Cocoa API if AppleScript fails
        Ok(Self::get_frontmost_app())
    }

    async fn is_idle(&self, threshold_seconds: u32) -> Result<bool> {
        let idle_seconds = Self::get_system_idle_seconds();
        Ok(idle_seconds > f64::from(threshold_seconds))
    }

    /// Get the current idle time in seconds
    async fn get_idle_seconds(&self) -> Result<u32> {
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let idle_secs = Self::get_system_idle_seconds() as u32;
        Ok(idle_secs)
    }

    async fn stop_monitoring(&mut self) -> Result<()> {
        log::info!("Stopped macOS activity monitoring");
        Ok(())
    }
}
