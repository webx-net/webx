use notify::{self, Error, Event, Watcher};
use std::path::{Path, PathBuf};
use std::sync::mpsc::Sender;
use std::time::{Duration, Instant};

use crate::engine::runtime::WXRuntimeMessage;
use crate::file::parser::parse_webx_file;
use crate::file::webx::WXModulePath;
use crate::reporting::debug::info;
use crate::reporting::warning::warning;
use crate::runner::WXMode;

struct FSWEvent {
    pub kind: notify::EventKind,
    pub path: WXModulePath,
    pub timestamp: Instant,
    is_empty_state: bool,
}

impl FSWEvent {
    fn new(kind: notify::EventKind, path: &Path) -> Self {
        Self {
            kind,
            path: WXModulePath::new(path.to_path_buf()),
            timestamp: Instant::now(),
            is_empty_state: false,
        }
    }

    fn empty() -> Self {
        Self {
            kind: notify::EventKind::default(),
            path: WXModulePath::new(PathBuf::default()),
            timestamp: Instant::now(),
            is_empty_state: true,
        }
    }

    fn is_duplicate(&self, earlier: &Self) -> bool {
        if self.is_empty_state || earlier.is_empty_state {
            return false;
        }
        const EPSILON: u128 = 100; // ms
        self.kind == earlier.kind
            && self.path == earlier.path
            && self.timestamp.duration_since(earlier.timestamp).as_millis() < EPSILON
    }
}

pub struct WXFileWatcher {}

impl WXFileWatcher {
    /// Registers the file watcher thread
    pub fn run(mode: WXMode, source_root: PathBuf, rt_tx: Sender<WXRuntimeMessage>) {
        let mut last_event: FSWEvent = FSWEvent::empty();
        let mut watcher = notify::recommended_watcher(move |res: Result<Event, Error>| {
            match res {
                Ok(event) => {
                    match event.kind {
                        notify::EventKind::Create(_) => {
                            let event = FSWEvent::new(event.kind, &event.paths[0]);
                            if !event.is_duplicate(&last_event) {
                                match parse_webx_file(&event.path.inner) {
                                    Ok(module) => {
                                        rt_tx.send(WXRuntimeMessage::New(module)).unwrap()
                                    }
                                    Err(e) => {
                                        warning(mode, format!("(FileWatcher) Error: {:?}", e))
                                    }
                                }
                            }
                            last_event = event; // Update last event
                        }
                        notify::EventKind::Modify(_) => {
                            let event = FSWEvent::new(event.kind, &event.paths[0]);
                            if !event.is_duplicate(&last_event) {
                                match parse_webx_file(&event.path.inner) {
                                    Ok(module) => rt_tx
                                        .send(WXRuntimeMessage::Swap(event.path.clone(), module))
                                        .unwrap(),
                                    Err(e) => {
                                        warning(mode, format!("(FileWatcher) Error: {:?}", e))
                                    }
                                }
                            }
                            last_event = event; // Update last event
                        }
                        notify::EventKind::Remove(_) => {
                            let event = FSWEvent::new(event.kind, &event.paths[0]);
                            if !event.is_duplicate(&last_event) {
                                rt_tx
                                    .send(WXRuntimeMessage::Remove(event.path.clone()))
                                    .unwrap();
                            }
                            last_event = event; // Update last event
                        }
                        _ => (),
                    }
                }
                Err(e) => warning(mode, format!("watch error: {:?}", e)),
            }
        })
        .unwrap();
        watcher
            .watch(&source_root, notify::RecursiveMode::Recursive)
            .unwrap();
        info(mode, "Hot reloading is enabled.");
        loop {
            std::thread::sleep(Duration::from_millis(1000));
        }
    }
}
