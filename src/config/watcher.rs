use anyhow::Result;
use notify_debouncer_mini::notify::RecursiveMode;
use notify_debouncer_mini::{new_debouncer, DebounceEventResult, DebouncedEventKind};
use std::path::{Path, PathBuf};
use std::time::Duration;
use tokio::sync::mpsc;
use tracing::{debug, warn};

/// Events emitted by the config watcher.
#[derive(Debug)]
pub enum ConfigEvent {
    /// The config file was modified on disk.
    Changed,
}

/// Watches a config file for changes and sends events via a channel.
pub struct ConfigWatcher {
    config_path: PathBuf,
}

impl ConfigWatcher {
    pub fn new(config_path: &Path) -> Self {
        Self {
            config_path: config_path.to_path_buf(),
        }
    }

    /// Start watching the config file. Returns a receiver of change events.
    /// The watcher runs on a background thread (via notify) and bridges to async
    /// via a tokio mpsc channel.
    pub fn watch(&self) -> Result<mpsc::Receiver<ConfigEvent>> {
        let (tx, rx) = mpsc::channel(16);

        let config_filename = self
            .config_path
            .file_name()
            .map(|f| f.to_os_string())
            .unwrap_or_default();

        let watch_dir = self
            .config_path
            .parent()
            .unwrap_or(Path::new("."))
            .to_path_buf();

        let mut debouncer = new_debouncer(
            Duration::from_millis(500),
            move |result: DebounceEventResult| match result {
                Ok(events) => {
                    let relevant = events.iter().any(|e| {
                        e.kind == DebouncedEventKind::Any
                            && e.path
                                .file_name()
                                .map(|f| f == config_filename)
                                .unwrap_or(false)
                    });
                    if relevant {
                        debug!("config file change detected");
                        let _ = tx.blocking_send(ConfigEvent::Changed);
                    }
                }
                Err(e) => {
                    warn!("config watcher error: {}", e);
                }
            },
        )?;

        debouncer
            .watcher()
            .watch(&watch_dir, RecursiveMode::NonRecursive)?;

        // Leak the debouncer to keep it alive for the lifetime of the process.
        // This is intentional — the watcher runs until the process exits.
        std::mem::forget(debouncer);

        Ok(rx)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    #[tokio::test]
    async fn watcher_detects_file_change() {
        let mut tmp = NamedTempFile::new().unwrap();
        writeln!(tmp, "[project]\nname = \"test\"").unwrap();

        let watcher = ConfigWatcher::new(tmp.path());
        let mut rx = watcher.watch().unwrap();

        // Give the watcher time to start
        tokio::time::sleep(Duration::from_millis(200)).await;

        // Modify the file
        writeln!(tmp, "\n[services.api]\ncommand = \"echo hi\"").unwrap();
        tmp.flush().unwrap();

        // Wait for the debounced event (500ms debounce + margin)
        let event = tokio::time::timeout(Duration::from_secs(3), rx.recv()).await;
        assert!(
            event.is_ok(),
            "should receive a change event within timeout"
        );
        match event.unwrap() {
            Some(ConfigEvent::Changed) => {} // expected
            other => panic!("expected ConfigEvent::Changed, got {:?}", other),
        }
    }
}
