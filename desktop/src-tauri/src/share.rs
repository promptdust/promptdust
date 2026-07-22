//! Native macOS Share sheet (`NSSharingServicePicker`) for the desktop app (#94).
//!
//! `share` hands a caller-composed **text** summary to the OS share sheet, so the target list
//! is exactly what the user has installed (Mail, Messages, AirDrop, Notes, …) — we maintain no
//! target list of our own. This is the **backend half**: the front-end wiring (composing a
//! finding's summary or the diagnostics bundle and invoking `share`) lands with the desktop
//! redesign port — there is no caller yet.
//!
//! The intended payload is *metadata* text — a finding's one-line summary (which may include
//! its filesystem path, reported metadata per PRIVACY.md) or the path-free redacted diagnostics
//! bundle. Conversation **content** must not flow here, and by construction can't: every command
//! the UI can invoke returns metadata-only data (the scan `OutputDocument`'s content-freeness is
//! enforced upstream by core's INV-3 CANARY test), so the UI has no content to compose in.
//! It is read-only w.r.t. scanned files; the command itself makes **no** network call (the OS
//! handles any send the user then chooses — INV-2 is scoped to the scan path, untouched here).
//!
//! The sheet is macOS-only; other platforms return an error so the front-end can fall back
//! (e.g. copy to clipboard).

/// Validate the text to share: trim it and reject empty/whitespace. Returns the trimmed slice.
fn validate_share_text(text: &str) -> Result<&str, String> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return Err("nothing to share".to_string());
    }
    Ok(trimmed)
}

/// Present the native share sheet for a caller-composed metadata summary, anchored to the
/// window. macOS only; other platforms return an error the front-end can fall back on.
///
/// INV-3: `text` is trusted to be metadata, on the same basis as `export_report`'s `contents`
/// (ADR-017) — every command the UI can invoke returns metadata-only data (CANARY-tested in
/// core), so the UI has no conversation content to compose into this string. A future command
/// that returns raw content would invalidate that and must not feed `share`.
#[tauri::command]
pub fn share(window: tauri::WebviewWindow, text: String) -> Result<(), String> {
    let text = validate_share_text(&text)?.to_string();
    present(&window, text)
}

#[cfg(target_os = "macos")]
fn present(window: &tauri::WebviewWindow, text: String) -> Result<(), String> {
    use objc2::rc::Retained;
    use objc2::runtime::AnyObject;
    use objc2::AnyThread;
    use objc2_app_kit::{NSSharingServicePicker, NSWindow};
    use objc2_foundation::{NSArray, NSPoint, NSRect, NSRectEdge, NSSize, NSString};

    // Fail fast (to the caller) if there's no native window to anchor to. `ns_window()` returns
    // `Err` when the window is gone; it never yields a null pointer.
    window
        .ns_window()
        .map_err(|_| "no native window to anchor the share sheet".to_string())?;
    let win = window.clone();

    window
        .run_on_main_thread(move || {
            // AppKit must be touched on the main thread; `run_on_main_thread` guarantees that.
            // Today a Tauri sync command already runs on the main thread, so this closure runs
            // inline — but fetching the NSWindow *here* (rather than capturing a raw pointer
            // before the call) keeps it correct without relying on that: the pointer is obtained
            // and used in one main-thread context, never held across a possible hop.
            let Ok(ns_window_ptr) = win.ns_window() else {
                return; // window gone → nothing to anchor to
            };
            // SAFETY: on the main thread (above); `ns_window_ptr` is the autoreleased NSWindow
            // Tauri just returned for this window — non-null and valid until the pool drains
            // after this closure returns. The `&NSWindow` never escapes the closure.
            let ns_window: &NSWindow = unsafe { &*(ns_window_ptr as *const NSWindow) };
            let Some(view) = ns_window.contentView() else {
                return;
            };
            let item: Retained<AnyObject> = NSString::from_str(&text).into_super().into_super();
            let items = NSArray::from_retained_slice(&[item]);
            // SAFETY: the items are NSStrings, which conform to NSPasteboardWriting.
            let picker = unsafe {
                NSSharingServicePicker::initWithItems(NSSharingServicePicker::alloc(), &items)
            };
            // Anchor a 1pt rect at the top-center of the content view; the sheet opens below it.
            let bounds = view.bounds();
            let rect = NSRect::new(
                NSPoint::new(bounds.size.width / 2.0, bounds.size.height),
                NSSize::new(1.0, 1.0),
            );
            // NSSharingServicePicker is documented to be shown "on mouseDown"; here it's driven
            // by an IPC round-trip rather than literally inside a mouse handler (fine in
            // practice — the manual smoke test should confirm it positions/behaves correctly).
            picker.showRelativeToRect_ofView_preferredEdge(rect, &view, NSRectEdge::MinY);
            // The picker must outlive this call while its popover is on screen — AppKit does not
            // retain it for us, so dropping it here would use-after-free the moment the user
            // picks a target. Leak it: the picker + its NSArray + an NSString copy of `text`,
            // **per share invocation** (the copy scales with `text`, so this assumes the intended
            // one-line summary). A delegate that releases on dismissal is the bounded refinement.
            std::mem::forget(picker);
        })
        .map_err(|e| e.to_string())
}

#[cfg(not(target_os = "macos"))]
fn present(_window: &tauri::WebviewWindow, _text: String) -> Result<(), String> {
    Err("the native share sheet is available on macOS only".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validate_share_text_trims_and_rejects_empty() {
        assert_eq!(validate_share_text("  hello  ").unwrap(), "hello");
        assert!(validate_share_text("").is_err());
        assert!(validate_share_text("   \n\t ").is_err());
        // A real one-line finding summary passes through unchanged (trimmed).
        let summary = "Cursor · ~/Library/Application Support/Cursor/state.vscdb · 44.7 MB · high";
        assert_eq!(validate_share_text(summary).unwrap(), summary);
    }

    // `share`/`present` take a live `WebviewWindow` and (on macOS) drive AppKit, so the sheet
    // itself is verified by the manual smoke test, not a unit test. `validate_share_text` above
    // is the pure, testable core; the non-macOS `present` is a one-line error fallback.
}
