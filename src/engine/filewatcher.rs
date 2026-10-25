use notify::{Event, EventKind, Watcher};
use std::{
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc::Sender,
        Arc,
    },
    time::Duration,
};

use crate::{
    engine::runtime::WXRuntimeMessage,
    reporting::{debug::info, warning::warning},
    runner::WXMode,
};

pub struct WXFileWatcher;

impl WXFileWatcher {
    pub fn run(
        mode: WXMode,
        source_root: PathBuf,
        rt_tx: Sender<WXRuntimeMessage>,
        running: Arc<AtomicBool>,
    ) {
        let mut watcher =
            notify::recommended_watcher(move |result: Result<Event, notify::Error>| {
                match result {
                    Ok(event)
                        if matches!(
                            event.kind,
                            EventKind::Create(_) | EventKind::Modify(_) | EventKind::Remove(_)
                        ) =>
                    {
                        // Events can contain both sides of a rename or a directory. The
                        // runtime reconciles the tree; never parse a half-written event.
                        let _ = rt_tx.send(WXRuntimeMessage::Reload);
                    }
                    Err(err) => warning(mode, format!("File watcher: {}", err)),
                    _ => {}
                }
            })
            .expect("Failed to create file watcher");
        watcher
            .watch(&source_root, notify::RecursiveMode::Recursive)
            .expect("Failed to watch source directory");
        info(mode, "Hot reloading is enabled.");
        while running.load(Ordering::SeqCst) {
            std::thread::sleep(Duration::from_millis(50));
        }
    }
}
