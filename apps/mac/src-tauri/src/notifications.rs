//! macOS completion alerts attributed to the running application bundle.
use block2::RcBlock;
use objc2_foundation::{NSError, NSString};
use objc2_user_notifications::{
    UNAuthorizationOptions, UNMutableNotificationContent, UNNotificationRequest,
    UNUserNotificationCenter,
};
use std::sync::atomic::{AtomicU64, Ordering};

fn center() -> Option<objc2::rc::Retained<UNUserNotificationCenter>> {
    let exe = std::env::current_exe().ok()?;
    exe.ancestors().any(|p| p.extension().is_some_and(|ext| ext == "app"))
        .then(UNUserNotificationCenter::currentNotificationCenter)
}

pub fn request_permission() {
    if let Some(center) = center() {
        let callback = RcBlock::new(|_granted: objc2::runtime::Bool, error: *mut NSError| {
            if !error.is_null() {
                eprintln!("Could not request Fabric Memory notification permission.");
            }
        });
        center.requestAuthorizationWithOptions_completionHandler(UNAuthorizationOptions::Alert, &callback);
    }
}

pub fn complete(success: bool) {
    let Some(center) = center() else { return; };
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let id = NSString::from_str(&format!("fabric-task-{}-{}", std::process::id(), NEXT.fetch_add(1, Ordering::Relaxed)));
    let content = UNMutableNotificationContent::new();
    content.setTitle(&NSString::from_str("Fabric Memory"));
    content.setBody(&NSString::from_str(if success {
        "Task complete."
    } else {
        "Task ended. Open Fabric Memory for details."
    }));
    let request = UNNotificationRequest::requestWithIdentifier_content_trigger(&id, &content, None);
    let callback = RcBlock::new(|error: *mut NSError| {
        if !error.is_null() {
            eprintln!("Fabric Memory could not post a task notification. Check macOS Notifications settings.");
        }
    });
    center.addNotificationRequest_withCompletionHandler(&request, Some(&callback));
}
