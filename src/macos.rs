use objc2::{AllocAnyThread, MainThreadMarker};
use objc2_app_kit::{NSApplication, NSImage};
use objc2_foundation::NSData;

/// Used when run without an .app bundle.
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
