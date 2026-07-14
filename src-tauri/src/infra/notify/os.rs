use tauri::AppHandle;
use tauri_plugin_notification::{NotificationExt, PermissionState};

use crate::error::{AppError, AppResult};

/// Shows a native desktop notification, requesting OS permission first if
/// it hasn't been granted yet. Best-effort by design — callers (
/// `AlertService`) log a warning on `Err` rather than letting one failed
/// sink break the rule engine loop.
pub fn send(app: &AppHandle, title: &str, body: &str) -> AppResult<()> {
    let notification = app.notification();

    let state = notification.permission_state()?;
    let state = if matches!(state, PermissionState::Granted) {
        state
    } else {
        notification.request_permission()?
    };
    if !matches!(state, PermissionState::Granted) {
        return Err(AppError::Internal(
            "desktop notification permission not granted".to_string(),
        ));
    }

    notification.builder().title(title).body(body).show()?;
    Ok(())
}
