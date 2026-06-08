use objc2::{AllocAnyThread, MainThreadMarker};
use objc2_app_kit::{
    NSApplication, NSDocumentController, NSImage, NSWindow, NSWindowCollectionBehavior,
};
use objc2_foundation::{NSData, NSString, NSURL};

/// Used when run without an .app bundle.
#[cfg_attr(feature = "app", allow(dead_code))]
pub fn set_dock_icon() {
    let icon_data = include_bytes!("../res/icons/icon.png");

    let data = NSData::with_bytes(icon_data);

    let Some(icon) = NSImage::initWithData(NSImage::alloc(), &data) else {
        log::error!("Failed to create NSImage from icon data");
        return;
    };

    let Some(mtm) = MainThreadMarker::new() else {
        log::error!("Cannot set dock icon: not on main thread");
        return;
    };

    let app = NSApplication::sharedApplication(mtm);

    // safety: the argument is always Some
    unsafe {
        app.setApplicationIconImage(Some(&icon));
    }
}

/// Ensure a newly created window appears on the active Space rather than
/// switching to whichever Space GG was previously on.
pub fn set_move_to_active_space(window: &tauri::WebviewWindow) {
    let Ok(ptr) = window.ns_window() else { return };
    let ns_win = unsafe { &*(ptr as *const NSWindow) };
    let behavior = ns_win.collectionBehavior();
    ns_win.setCollectionBehavior(behavior | NSWindowCollectionBehavior::MoveToActiveSpace);
}

pub fn remove_move_to_active_space(window: &tauri::Window) {
    let Ok(ptr) = window.ns_window() else { return };
    let ns_win = unsafe { &*(ptr as *const NSWindow) };
    let behavior = ns_win.collectionBehavior();
    ns_win.setCollectionBehavior(behavior & !NSWindowCollectionBehavior::MoveToActiveSpace);
}

pub fn activate_app() {
    let Some(mtm) = MainThreadMarker::new() else {
        log::error!("Cannot activate app: not on main thread");
        return;
    };

    let app = NSApplication::sharedApplication(mtm);
    #[allow(deprecated)]
    app.activateIgnoringOtherApps(true);
}

/// add a workspace path to the macos "Recent Items" dock menu
pub fn note_recent_document(workspace_path: String) {
    let Some(mtm) = MainThreadMarker::new() else {
        log::error!("Failed to add recent document: not on main thread");
        return;
    };

    let path_str = NSString::from_str(&workspace_path);
    let url = NSURL::fileURLWithPath(&path_str);

    let controller = NSDocumentController::sharedDocumentController(mtm);
    controller.noteNewRecentDocumentURL(&url);
}
