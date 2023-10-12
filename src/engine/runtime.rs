use std::{sync::mpsc::Receiver, net::{SocketAddr, TcpListener}, io::{Read, Write}};

use crate::{file::webx::{WXModule, WXModulePath}, runner::WXMode, reporting::{debug::info, warning::warning}};

pub enum WXRuntimeMessage {
    NewModule(WXModule),
    SwapModule(WXModulePath, WXModule),
    RemoveModule(WXModulePath),
}
pub struct WXRuntime {
    modules: Vec<WXModule>,
    modules_rx: Receiver<WXRuntimeMessage>,
    mode: WXMode,
}

impl WXRuntime {
    pub fn new(rx: Receiver<WXRuntimeMessage>, mode: WXMode) -> Self {
        WXRuntime {
            modules: Vec::new(),
            modules_rx: rx,
            mode
        }
    }

    pub fn load_modules(&mut self, modules: Vec<WXModule>) {
        self.modules.extend(modules.into_iter());
    }

    pub fn run(&mut self) {
        info(self.mode, "Runtime started, waiting for module updates and HTTP requests");
        let addrs = [
            SocketAddr::from(([127, 0, 0, 1], 8080)), // TODO: Only in dev mode
            SocketAddr::from(([127, 0, 0, 1], 80)),   // TODO: Only in prod mode
            SocketAddr::from(([127, 0, 0, 1], 443)),  // TODO: Only in prod mode
        ];
        let listener = TcpListener::bind(&addrs[..]).unwrap();
        // Don't block if in dev mode, wait and read hotswap messages.
        listener.set_nonblocking(self.mode == WXMode::Dev).unwrap();
        loop {
            self.listen_for_requests(&listener);
            // In case we are in dev mode, we don't want the TCP listener to block the thread.
            // Instead, we want to sleep for a short while and then check for new messages
            // from the channel repeatedly in case we have received a new module hotswap.
            if self.mode == WXMode::Dev {
                self.sync_channel_messages();
                std::thread::sleep(std::time::Duration::from_millis(100));
            }
        }
    }

    /// Look for module updates from the given channel.
    /// This function is **non-blocking**.
    /// All queued updates are applied immediately.
    fn sync_channel_messages(&mut self) {
        while let Ok(msg) = self.modules_rx.try_recv() {
            match msg {
                WXRuntimeMessage::NewModule(module) => {
                    info(self.mode, &format!("(Runtime) New module: {}", module.path.module_name()));
                    self.modules.push(module);
                },
                WXRuntimeMessage::SwapModule(path, module) => {
                    info(self.mode, &format!("(Runtime) Reloaded module: {}", module.path.module_name()));
                    self.modules.retain(|m| m.path != path);
                    self.modules.push(module);
                },
                WXRuntimeMessage::RemoveModule(path) => {
                    info(self.mode, &format!("(Runtime) Removed module: {}", path.module_name()));
                    self.modules.retain(|m| m.path != path);
                }
            }
        }
    }

    fn listen_for_requests(&self, listener: &TcpListener) {
        if let Ok((mut stream, addr)) = listener.accept() {
            info(self.mode, &format!("Runtime received request from {}", addr));
            let mut buf = [0; 1024];
            if let Ok(_) = stream.read(&mut buf) {
                let response = b"HTTP/1.1 200 OK\r\n\r\nHello, world!";
                stream.write(response).unwrap();
                stream.flush().unwrap();
            } else { warning(format!("Failed to read request from {}", addr)); }
        }
    }
}
        