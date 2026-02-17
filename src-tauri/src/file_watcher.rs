/// File watcher for CharTracker.lua SavedVariables.
/// Uses notify crate with debouncing to detect file changes
/// and trigger sync automatically.

use notify::RecursiveMode;
use notify_debouncer_mini::{new_debouncer, DebouncedEventKind};
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::time::Duration;

pub enum WatcherEvent {
    FileChanged(PathBuf),
    Error(String),
}

/// Start watching a directory for changes to CharTracker.lua.
/// Returns a receiver that emits events when the file changes.
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
    let target_file = "CharTracker.lua".to_lowercase();

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

                            if file_name.as_deref() == Some(&target_file) {
                                let _ =
                                    sender.send(WatcherEvent::FileChanged(event.path.clone()));
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
