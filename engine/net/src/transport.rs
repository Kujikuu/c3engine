//! TCP connections run by tokio on a background thread, seen by the game as queues: what
//! arrived is polled, what to send is queued. Each connection decrypts one packet at a time
//! (header, then the rest) so a cipher that re-keys on a packet affects exactly the bytes
//! after it.

use crate::Plain;
use crate::cipher::Cipher;
use crate::framing::{FrameError, Framing, LengthPrefixed};
use std::collections::HashMap;
use std::io;
use std::net::{SocketAddr, ToSocketAddrs};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, mpsc};
use std::thread::JoinHandle;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::net::tcp::{OwnedReadHalf, OwnedWriteHalf};
use tokio::sync::{mpsc as tmpsc, oneshot};

/// How a connection's bytes are shaped: its framing and its cipher.
pub struct Wire {
    pub framing: Box<dyn Framing + Sync>,
    pub cipher: Box<dyn Cipher>,
    /// A packet sent first, as soon as the connection is up, before anything queued (a
    /// server's key exchange). It is sealed and encrypted like any other.
    pub greeting: Option<Vec<u8>>,
}

impl Wire {
    /// Length-prefixed packets, unencrypted (the engine's own protocol).
    pub fn plain() -> Self {
        Self {
            framing: Box::new(LengthPrefixed),
            cipher: Box::new(Plain),
            greeting: None,
        }
    }
}

/// A connection to a server, unique while the server runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ConnId(pub u64);

/// What happened on a server's connections.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NetEvent {
    Connected {
        id: ConnId,
        addr: SocketAddr,
    },
    Received {
        id: ConnId,
        packet: Vec<u8>,
    },
    /// The connection is gone; `reason` says why when it wasn't a clean close.
    Closed {
        id: ConnId,
        reason: Option<String>,
    },
}

/// What happened on a client's connection.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClientEvent {
    Received(Vec<u8>),
    Closed(Option<String>),
}

enum Outgoing {
    Packet(Vec<u8>),
    Close,
}

type Senders = Arc<Mutex<HashMap<ConnId, tmpsc::UnboundedSender<Outgoing>>>>;

/// Reads packets until the peer closes, `stop` fires, or the stream breaks; returns why it
/// ended (`None`: a clean close).
async fn read_packets(
    mut rd: OwnedReadHalf,
    framing: Arc<dyn Framing + Sync>,
    cipher: Arc<Mutex<Box<dyn Cipher>>>,
    mut stop: oneshot::Receiver<()>,
    mut deliver: impl FnMut(Vec<u8>),
) -> Option<String> {
    let mut raw: Vec<u8> = Vec::new();
    // Bytes at the start of `raw` already decrypted (the next packet's header).
    let mut decrypted = 0;
    let mut chunk = vec![0u8; 16 * 1024];
    loop {
        let n = tokio::select! {
            read = rd.read(&mut chunk) => match read {
                Ok(0) => return None,
                Ok(n) => n,
                Err(e) => return Some(e.to_string()),
            },
            _ = &mut stop => return None,
        };
        raw.extend_from_slice(&chunk[..n]);
        loop {
            let head = framing.header_len();
            if raw.len() < head {
                break;
            }
            if decrypted < head {
                cipher
                    .lock()
                    .expect("cipher")
                    .decrypt(&mut raw[decrypted..head]);
                decrypted = head;
            }
            let len = match framing.frame_len(&raw[..head]) {
                Ok(len) if len < head => return Some(FrameError::TooSmall(len).to_string()),
                Ok(len) => len,
                Err(e) => return Some(e.to_string()),
            };
            if raw.len() < len {
                break;
            }
            let frame: Vec<u8> = {
                let mut c = cipher.lock().expect("cipher");
                c.decrypt(&mut raw[decrypted..len]);
                let frame: Vec<u8> = raw.drain(..len).collect();
                c.received(&frame);
                frame
            };
            decrypted = 0;
            deliver(framing.open(frame));
        }
    }
}

/// Sends queued packets until told to close or the queue ends.
async fn write_packets(
    mut wr: OwnedWriteHalf,
    framing: Arc<dyn Framing + Sync>,
    cipher: Arc<Mutex<Box<dyn Cipher>>>,
    mut queue: tmpsc::UnboundedReceiver<Outgoing>,
    greeting: Option<Vec<u8>>,
    stop_reading: oneshot::Sender<()>,
) {
    if let Some(packet) = greeting {
        let mut bytes = framing.seal(packet);
        cipher.lock().expect("cipher").encrypt(&mut bytes);
        if wr.write_all(&bytes).await.is_err() {
            let _ = stop_reading.send(());
            return;
        }
    }
    while let Some(out) = queue.recv().await {
        match out {
            Outgoing::Packet(packet) => {
                let mut bytes = framing.seal(packet);
                cipher.lock().expect("cipher").encrypt(&mut bytes);
                if wr.write_all(&bytes).await.is_err() {
                    break;
                }
            }
            Outgoing::Close => break,
        }
    }
    let _ = wr.shutdown().await;
    let _ = stop_reading.send(());
}

/// Runs one connection: reading delivers packets, the queue sends them. Returns why it
/// ended.
async fn run_connection(
    stream: TcpStream,
    wire: Wire,
    queue: tmpsc::UnboundedReceiver<Outgoing>,
    deliver: impl FnMut(Vec<u8>),
) -> Option<String> {
    let _ = stream.set_nodelay(true);
    let (rd, wr) = stream.into_split();
    let framing: Arc<dyn Framing + Sync> = Arc::from(wire.framing);
    let cipher = Arc::new(Mutex::new(wire.cipher));
    let (stop_tx, stop_rx) = oneshot::channel();
    let writer = tokio::spawn(write_packets(
        wr,
        framing.clone(),
        cipher.clone(),
        queue,
        wire.greeting,
        stop_tx,
    ));
    let reason = read_packets(rd, framing, cipher, stop_rx, deliver).await;
    writer.abort();
    reason
}

fn runtime() -> io::Result<tokio::runtime::Runtime> {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
}

/// Accepts connections and exchanges packets with them.
pub struct Server {
    addr: SocketAddr,
    events: mpsc::Receiver<NetEvent>,
    senders: Senders,
    shutdown: Option<oneshot::Sender<()>>,
    thread: Option<JoinHandle<()>>,
}

impl Server {
    /// Listens on `addr` (port 0 picks a free one); `wire` shapes each new connection.
    pub fn bind(
        addr: impl ToSocketAddrs,
        wire: impl Fn() -> Wire + Send + 'static,
    ) -> io::Result<Self> {
        let listener = std::net::TcpListener::bind(addr)?;
        listener.set_nonblocking(true)?;
        let addr = listener.local_addr()?;
        let rt = runtime()?;
        let (events_tx, events) = mpsc::channel();
        let senders: Senders = Arc::default();
        let (shutdown, mut stop) = oneshot::channel::<()>();
        let conns = senders.clone();
        let thread = std::thread::Builder::new()
            .name(format!("c3-net server {addr}"))
            .spawn(move || {
                rt.block_on(async move {
                    let listener = match tokio::net::TcpListener::from_std(listener) {
                        Ok(l) => l,
                        Err(_) => return,
                    };
                    let next = AtomicU64::new(1);
                    loop {
                        let (stream, peer) = tokio::select! {
                            accepted = listener.accept() => match accepted {
                                Ok(a) => a,
                                Err(_) => continue,
                            },
                            _ = &mut stop => break,
                        };
                        let id = ConnId(next.fetch_add(1, Ordering::Relaxed));
                        let (tx, rx) = tmpsc::unbounded_channel();
                        conns.lock().expect("connections").insert(id, tx);
                        let events = events_tx.clone();
                        let conns = conns.clone();
                        let _ = events.send(NetEvent::Connected { id, addr: peer });
                        let wire = wire();
                        tokio::spawn(async move {
                            let sink = events.clone();
                            let deliver = move |packet| {
                                let _ = sink.send(NetEvent::Received { id, packet });
                            };
                            let reason = run_connection(stream, wire, rx, deliver).await;
                            conns.lock().expect("connections").remove(&id);
                            let _ = events.send(NetEvent::Closed { id, reason });
                        });
                    }
                });
            })?;
        Ok(Self {
            addr,
            events,
            senders,
            shutdown: Some(shutdown),
            thread: Some(thread),
        })
    }

    /// The address it listens on.
    pub fn local_addr(&self) -> SocketAddr {
        self.addr
    }

    /// What happened since the last call.
    pub fn poll(&self) -> Vec<NetEvent> {
        self.events.try_iter().collect()
    }

    /// Queues `packet` for connection `id` (dropped if it is gone).
    pub fn send(&self, id: ConnId, packet: Vec<u8>) {
        if let Some(tx) = self.senders.lock().expect("connections").get(&id) {
            let _ = tx.send(Outgoing::Packet(packet));
        }
    }

    /// Closes connection `id` once what was queued for it is sent.
    pub fn close(&self, id: ConnId) {
        if let Some(tx) = self.senders.lock().expect("connections").get(&id) {
            let _ = tx.send(Outgoing::Close);
        }
    }

    /// How many connections are open.
    pub fn connections(&self) -> usize {
        self.senders.lock().expect("connections").len()
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        if let Some(stop) = self.shutdown.take() {
            let _ = stop.send(());
        }
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

/// A connection to a server.
pub struct Client {
    events: mpsc::Receiver<ClientEvent>,
    queue: tmpsc::UnboundedSender<Outgoing>,
    open: Arc<AtomicBool>,
    shutdown: Option<oneshot::Sender<()>>,
    thread: Option<JoinHandle<()>>,
}

impl Client {
    /// Connects to `addr` (blocking until connected or refused).
    pub fn connect(addr: impl ToSocketAddrs, wire: Wire) -> io::Result<Self> {
        let stream = std::net::TcpStream::connect(addr)?;
        stream.set_nonblocking(true)?;
        let rt = runtime()?;
        let (events_tx, events) = mpsc::channel();
        let (queue, rx) = tmpsc::unbounded_channel();
        let open = Arc::new(AtomicBool::new(true));
        let (shutdown, stop) = oneshot::channel::<()>();
        let still_open = open.clone();
        let thread = std::thread::Builder::new()
            .name("c3-net client".into())
            .spawn(move || {
                rt.block_on(async move {
                    let Ok(stream) = TcpStream::from_std(stream) else {
                        return;
                    };
                    let sink = events_tx.clone();
                    let deliver = move |packet| {
                        let _ = sink.send(ClientEvent::Received(packet));
                    };
                    let reason = tokio::select! {
                        reason = run_connection(stream, wire, rx, deliver) => reason,
                        _ = stop => None,
                    };
                    still_open.store(false, Ordering::Relaxed);
                    let _ = events_tx.send(ClientEvent::Closed(reason));
                });
            })?;
        Ok(Self {
            events,
            queue,
            open,
            shutdown: Some(shutdown),
            thread: Some(thread),
        })
    }

    /// What arrived since the last call.
    pub fn poll(&self) -> Vec<ClientEvent> {
        self.events.try_iter().collect()
    }

    /// Queues `packet` for the server.
    pub fn send(&self, packet: Vec<u8>) {
        let _ = self.queue.send(Outgoing::Packet(packet));
    }

    /// Whether the connection is still up.
    pub fn is_open(&self) -> bool {
        self.open.load(Ordering::Relaxed)
    }
}

impl Drop for Client {
    fn drop(&mut self) {
        let _ = self.queue.send(Outgoing::Close);
        if let Some(stop) = self.shutdown.take() {
            let _ = stop.send(());
        }
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}
