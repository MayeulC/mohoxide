//! The system clipboard: FAF's `CopyToClipboard`, and paste / copy in edit
//! boxes (an internet join code is 52 characters: nobody types that).
//!
//! One `Clipboard` lives for the whole process. On Linux the copied text is
//! served by the `Clipboard` that set it (X11, and Wayland compositors
//! without the data-control protocol, through XWayland): one dropped right
//! after the copy took the text with it, so nothing could be pasted.
//! On Wayland compositors with data-control (sway and the other wlroots
//! ones, KDE) arboard's `wayland-data-control` backend is used instead.

use std::sync::Mutex;

static CLIPBOARD: Mutex<Option<arboard::Clipboard>> = Mutex::new(None);

/// Runs `f` on the process's clipboard, opening it on first use.
fn with<R>(f: impl FnOnce(&mut arboard::Clipboard) -> Result<R, arboard::Error>) -> Result<R, arboard::Error> {
    let mut clipboard = CLIPBOARD.lock().unwrap_or_else(|e| e.into_inner());
    if clipboard.is_none() {
        *clipboard = Some(arboard::Clipboard::new()?);
    }
    f(clipboard.as_mut().expect("opened above"))
}

/// The clipboard's text, if it holds any.
pub fn get() -> Option<String> {
    with(|c| c.get_text()).map_err(|e| tracing::debug!(error = %e, "clipboard: nothing to paste")).ok()
}

/// Puts `text` on the clipboard; false if the system refused.
pub fn set(text: &str) -> bool {
    with(|c| c.set_text(text.to_owned())).map_err(|e| tracing::warn!(error = %e, "clipboard: copy failed")).is_ok()
}

/// Closes the clipboard as the application exits (arboard asks for every
/// `Clipboard` to be dropped first; on X11 that hands what we copied to a
/// clipboard manager, if one is running, so it outlives us).
pub fn release() {
    CLIPBOARD.lock().unwrap_or_else(|e| e.into_inner()).take();
}
