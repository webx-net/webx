use std::{sync::mpsc::Receiver, path::PathBuf, net::{SocketAddr, TcpListener}, io::{Read, Write}};

use crate::{file::webx::WXModule, runner::WXMode};

pub enum WXRuntimeMessage {
    NewModule(WXModule),
    SwapModule(PathBuf, WXModule),
    RemoveModule(PathBuf),
    Info(String),
    Exit,
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
        println!("Runtime started, waiting for module updates and HTTP requests");
        let addrs = [
            SocketAddr::from(([127, 0, 0, 1], 8080)), // TODO: Only in dev mode
            SocketAddr::from(([127, 0, 0, 1], 80)),   // TODO: Only in prod mode
            SocketAddr::from(([127, 0, 0, 1], 443)),  // TODO: Only in prod mode
        ];
        let listener = TcpListener::bind(&addrs[..]).unwrap();
        let manual_blocking = self.mode == WXMode::Dev;
        listener.set_nonblocking(manual_blocking).unwrap();
        loop {
            if !self.sync_channel_messages() { break } // Exit if requested
            // Listen for requests
            if let Ok((mut stream, addr)) = listener.accept() /* Blocking */ {
                println!("Runtime received request from {}", addr);
                let mut buf = [0; 1024];
                stream.read(&mut buf).unwrap();
                let response = b"HTTP/1.1 200 OK\r\n\r\nHello, world!";
                stream.write(response).unwrap();
                stream.flush().unwrap();
            }
            // In case we are in dev mode, we don't want the TCP listener to block the thread.
            // Instead, we want to sleep for a short while and then check for new messages
            // from the channel repeatedly in case we have received a new module hotswap.
            if manual_blocking { std::thread::sleep(std::time::Duration::from_millis(100)); }
        }
    }

    /// Look for module updates from the given channel.
    /// This function is non-blocking.
    /// All queued updates are applied immediately.
    fn sync_channel_messages(&mut self) -> bool {
        while let Ok(msg) = self.modules_rx.try_recv() /* Non-blocking */ {
            match msg {
                WXRuntimeMessage::NewModule(module) => {
                    println!("Runtime received new module");
                    self.modules.push(module);
                },
                WXRuntimeMessage::SwapModule(path, module) => {
                    println!("Runtime received swap module");
                    self.modules.retain(|m| m.path.inner != path);
                    self.modules.push(module);
                },
                WXRuntimeMessage::RemoveModule(path) => {
                    println!("Runtime received remove module");
                    self.modules.retain(|m| m.path.inner != path);
                },
                WXRuntimeMessage::Info(text) => {
                    println!("Runtime received info: {}", text);
                },
                WXRuntimeMessage::Exit => {
                    println!("Runtime received exit");
                    return false;
                }
            }
        }
        true
    }
}
        