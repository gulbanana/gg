use objc2::runtime::AnyObject;
use objc2::{AllocAnyThread, MainThreadMarker};
use objc2_app_kit::{
    NSApplication, NSDocumentController, NSFont, NSFontAttributeName, NSImage, NSStringDrawing,
    NSTextField, NSWindow, NSWindowButton, NSWindowCollectionBehavior, NSWindowStyleMask,
};
use objc2_foundation::{
    NSData, NSDictionary, NSOperatingSystemVersion, NSPoint, NSProcessInfo, NSRect, NSSize,
    NSString, NSURL,
};

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

/// macOS 26+, whose titlebar leaves room for content by left-aligning the title.
pub fn is_tahoe_or_later() -> bool {
    let tahoe = NSOperatingSystemVersion {
        majorVersion: 26,
        minorVersion: 0,
        patchVersion: 0,
    };
    NSProcessInfo::processInfo().isOperatingSystemAtLeastVersion(tahoe)
}

/// Height of a standard titlebar, which varies by OS version and linked SDK.
pub fn titlebar_height() -> f64 {
    let Some(mtm) = MainThreadMarker::new() else {
        log::error!("Cannot measure titlebar: not on main thread");
        return 0.0;
    };

    let content = NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(100.0, 100.0));
    let frame =
        NSWindow::frameRectForContentRect_styleMask(content, NSWindowStyleMask::Titled, mtm);
    frame.size.height - content.size.height
}

/// The first title which would end before `limit` (in window coordinates), or else the last.
/// None if the titlebar can't be measured.
pub fn fit_title<'a>(
    window: &tauri::Window,
    titles: &'a [String],
    limit: f64,
) -> Option<&'a String> {
    // leaves a gap before whatever follows, beyond the field's own text inset
    const MARGIN: f64 = 12.0;

    let ptr = window.ns_window().ok()?;
    let ns_win = unsafe { &*(ptr as *const NSWindow) };

    // appkit's title field lives alongside the traffic lights
    let titlebar = unsafe {
        ns_win
            .standardWindowButton(NSWindowButton::CloseButton)?
            .superview()?
    };
    let field = titlebar
        .subviews()
        .iter()
        .find_map(|view| view.downcast::<NSTextField>().ok())?;
    let origin = field.convertRect_toView(field.bounds(), None).origin.x;

    let font = NSFont::titleBarFontOfSize(0.0);
    let font: &AnyObject = &font;
    let attributes = NSDictionary::from_slices(&[unsafe { NSFontAttributeName }], &[font]);

    titles
        .iter()
        .find(|title| {
            let width =
                unsafe { NSString::from_str(title).sizeWithAttributes(Some(&attributes)) }.width;
            origin + width + MARGIN <= limit
        })
        .or(titles.last())
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
