/// File watcher for SavedVariables (CharTracker.lua and Gargul.lua).
/// Uses notify crate with debouncing to detect file changes
/// and trigger sync automatically.

use notify::RecursiveMode;
use notify_debouncer_mini::{new_debouncer, DebouncedEventKind};
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::time::Duration;

pub enum WatcherEvent {
    FileChanged(PathBuf),
    GargulChanged(PathBuf),
    Error(String),
}

/// Start watching a directory for changes to CharTracker.lua and Gargul.lua.
/// Returns a receiver that emits events when either file changes.
/// The watcher handle must be kept alive (dropping it stops watching).
pub fn start_watching(
    saved_variables_dir: &Path,
) -> Result<
    (
        mpsc::Receiver<WatcherEvent>,
        notify_debouncer_mini::Debouncer<notify::RecommendedWatcher>,
    ),
    String,
> {
    let (tx, rx) = mpsc::channel();
    let chartracker_file = "chartracker.lua";
    let gargul_file = "gargul.lua";

    let sender = tx.clone();
    let mut debouncer = new_debouncer(
        Duration::from_secs(3),
        move |result: Result<Vec<notify_debouncer_mini::DebouncedEvent>, notify::Error>| {
            match result {
                Ok(events) => {
                    for event in events {
                        if event.kind == DebouncedEventKind::Any {
                            let file_name = event
                                .path
                                .file_name()
                                .map(|n| n.to_string_lossy().to_lowercase());

                            if let Some(name) = file_name.as_deref() {
                                if name == chartracker_file {
                                    let _ = sender
                                        .send(WatcherEvent::FileChanged(event.path.clone()));
                                } else if name == gargul_file {
                                    let _ = sender
                                        .send(WatcherEvent::GargulChanged(event.path.clone()));
                                }
                            }
                        }
                    }
                }
                Err(e) => {
                    let _ = sender.send(WatcherEvent::Error(e.to_string()));
                }
            }
        },
    )
    .map_err(|e| format!("Failed to create file watcher: {}", e))?;

    debouncer
        .watcher()
        .watch(saved_variables_dir, RecursiveMode::NonRecursive)
        .map_err(|e| format!("Failed to watch directory: {}", e))?;

    Ok((rx, debouncer))
}
