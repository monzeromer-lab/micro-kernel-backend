//! File-system watcher that monitors a folder for `.wasm` module changes.
//!
//! Uses the `notify` crate to receive filesystem events and emit a stream
//! of module names that should be reloaded.

use notify::{Event, EventKind, RecursiveMode, Watcher};
use std::path::Path;
use std::sync::mpsc::{self, Receiver};

// ---------------------------------------------------------------------------
// ModuleWatcher
// ---------------------------------------------------------------------------

/// Watches a directory for `.wasm` files being created, modified, or removed.
///
/// ```rust,ignore
/// let mut watcher = ModuleWatcher::start("./modules")?;
/// while let Ok(name) = watcher.rx.recv() {
///     match name {
///         WatchEvent::Added("user") => register_module("user"),
///         WatchEvent::Modified("user") => reload_module("user"),
///         WatchEvent::Removed("user") => unload_module("user"),
///     }
/// }
/// ```
pub struct ModuleWatcher {
    /// The underlying `notify` watcher handle — kept alive so it keeps watching.
    _watcher: notify::RecommendedWatcher,
    /// Channel receiver for filesystem events, parsed into [`WatchEvent`]s.
    pub rx: Receiver<WatchEvent>,
}

/// A parsed filesystem event for a WASM module.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WatchEvent {
    /// A new `.wasm` file appeared.
    Added(String),
    /// An existing `.wasm` file was modified.
    Modified(String),
    /// A `.wasm` file was removed.
    Removed(String),
}

impl ModuleWatcher {
    /// Start watching `dir` for `.wasm` files.
    ///
    /// Returns immediately — events are delivered via `self.rx`.
    pub fn start(dir: impl AsRef<Path>) -> Result<Self, Box<dyn std::error::Error>> {
        let dir = dir.as_ref().to_path_buf();

        // Ensure the directory exists
        if !dir.exists() {
            std::fs::create_dir_all(&dir)?;
        }

        let (tx, rx) = mpsc::channel();

        let mut watcher = notify::recommended_watcher(move |res: Result<Event, notify::Error>| {
            if let Ok(event) = res {
                for path in &event.paths {
                    if let Some(name) = extract_module_name(path) {
                        let watch_event = match event.kind {
                            EventKind::Create(_) => WatchEvent::Added(name),
                            EventKind::Modify(_) => WatchEvent::Modified(name),
                            EventKind::Remove(_) => WatchEvent::Removed(name),
                            _ => continue, // skip access/other events
                        };
                        let _ = tx.send(watch_event);
                    }
                }
            }
        })?;

        watcher.watch(&dir, RecursiveMode::NonRecursive)?;

        Ok(Self {
            _watcher: watcher,
            rx,
        })
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Extract a valid module name from a `.wasm` file path.
///
/// Rules (as specified):
/// - Lowercase only
/// - No special characters
/// - No numbers
/// - Just a string (a–z)
///
/// Returns `None` if the file doesn't match the naming convention.
fn extract_module_name(path: &Path) -> Option<String> {
    let stem = path.file_stem()?.to_str()?;

    // Must end with .wasm
    if path.extension()?.to_str()? != "wasm" {
        return None;
    }

    // Validate: only lowercase a–z
    if stem.is_empty() || !stem.chars().all(|c| c.is_ascii_lowercase()) {
        return None;
    }

    Some(stem.to_owned())
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_valid_module_names() {
        assert_eq!(extract_module_name(Path::new("user.wasm")), Some("user".into()));
        assert_eq!(extract_module_name(Path::new("product.wasm")), Some("product".into()));
        assert_eq!(extract_module_name(Path::new("a.wasm")), Some("a".into()));
    }

    #[test]
    fn test_invalid_module_names() {
        assert_eq!(extract_module_name(Path::new("User.wasm")), None); // uppercase
        assert_eq!(extract_module_name(Path::new("user1.wasm")), None); // number
        assert_eq!(extract_module_name(Path::new("user_api.wasm")), None); // underscore
        assert_eq!(extract_module_name(Path::new("user-api.wasm")), None); // hyphen
        assert_eq!(extract_module_name(Path::new("user.txt")), None); // wrong extension
        assert_eq!(extract_module_name(Path::new("")), None);
    }
}
