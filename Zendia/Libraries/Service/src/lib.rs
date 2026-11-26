/*
 * Copyright (c) 2026-present, the Zendia developers.
 *
 * SPDX-License-Identifier: BSD-2-Clause
 */

//! The template every Zendia service process is built on: its IPC framing, protocol version handshake, launch from
//! the UI process, entering its sandbox, and restart after a crash.
//!
//! See `Zendia/Documentation/Architecture.md`, "Processes".

use serde_json::{Value, json};
use std::collections::HashMap;
use std::io::{self, Read, Write};
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
use std::os::unix::net::UnixStream;
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, mpsc};
use std::time::Duration;

/// Environment variable carrying the inherited UI-process IPC socket descriptor in every Zendia service.
pub const ENV_UI_SOCKET_FD: &str = "ZENDIA_UI_FD";
/// Environment variable carrying the optional inherited Hub embedder-channel socket descriptor.
pub const ENV_EMBEDDER_SOCKET_FD: &str = "ZENDIA_EMBEDDER_FD";

/// Maximum frame body size (64 MiB — large enough for extension packages while bounding allocations).
pub const MAX_FRAME_BYTES: usize = 64 * 1024 * 1024;

/// Default timeout for synchronous request/response exchanges over a service channel.
pub const DEFAULT_SYNC_TIMEOUT: Duration = Duration::from_secs(15);

/// Wire discriminant for one [`Frame`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum FrameKind {
    Request = 0,
    Response = 1,
    Event = 2,
}

impl FrameKind {
    pub fn from_byte(byte: u8) -> io::Result<Self> {
        match byte {
            0 => Ok(Self::Request),
            1 => Ok(Self::Response),
            2 => Ok(Self::Event),
            other => Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("unknown service frame kind {other}"),
            )),
        }
    }
}

/// One framed message exchanged over a Zendia service or Hub embedder channel.
///
/// Binary layout on the wire (little-endian):
/// `[u32 body_len][u8 kind][u64 id][u32 json_len][json_bytes][binary_bytes]`
/// where `body_len = 1 + 8 + 4 + json_len + binary_bytes.len()`.
#[derive(Clone, Debug, PartialEq)]
pub struct Frame {
    pub kind: FrameKind,
    pub id: u64,
    pub json: Value,
    pub binary: Vec<u8>,
}

impl Frame {
    pub fn request(id: u64, json: Value) -> Self {
        Self {
            kind: FrameKind::Request,
            id,
            json,
            binary: Vec::new(),
        }
    }

    pub fn request_with_binary(id: u64, json: Value, binary: Vec<u8>) -> Self {
        Self {
            kind: FrameKind::Request,
            id,
            json,
            binary,
        }
    }

    pub fn response_ok(id: u64, data: Value) -> Self {
        Self {
            kind: FrameKind::Response,
            id,
            json: json!({ "ok": true, "data": data }),
            binary: Vec::new(),
        }
    }

    pub fn response_ok_with_binary(id: u64, data: Value, binary: Vec<u8>) -> Self {
        Self {
            kind: FrameKind::Response,
            id,
            json: json!({ "ok": true, "data": data }),
            binary,
        }
    }

    pub fn response_err(id: u64, error: impl Into<String>) -> Self {
        Self {
            kind: FrameKind::Response,
            id,
            json: json!({ "ok": false, "error": error.into() }),
            binary: Vec::new(),
        }
    }

    pub fn event(json: Value) -> Self {
        Self {
            kind: FrameKind::Event,
            id: 0,
            json,
            binary: Vec::new(),
        }
    }

    pub fn event_with_binary(json: Value, binary: Vec<u8>) -> Self {
        Self {
            kind: FrameKind::Event,
            id: 0,
            json,
            binary,
        }
    }

    pub fn op(&self) -> Option<&str> {
        self.json.get("op").and_then(Value::as_str)
    }
}

/// Writes one framed message atomically to `writer`.
pub fn write_frame<W: Write>(writer: &mut W, frame: &Frame) -> io::Result<()> {
    let json_bytes =
        serde_json::to_vec(&frame.json).map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
    let body_len = 1usize
        .checked_add(8)
        .and_then(|n| n.checked_add(4))
        .and_then(|n| n.checked_add(json_bytes.len()))
        .and_then(|n| n.checked_add(frame.binary.len()))
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "frame size overflow"))?;
    if body_len > MAX_FRAME_BYTES {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("frame exceeds {MAX_FRAME_BYTES} byte limit"),
        ));
    }

    let mut header = [0u8; 4 + 1 + 8 + 4];
    header[0..4].copy_from_slice(&(body_len as u32).to_le_bytes());
    header[4] = frame.kind as u8;
    header[5..13].copy_from_slice(&frame.id.to_le_bytes());
    header[13..17].copy_from_slice(&(json_bytes.len() as u32).to_le_bytes());

    writer.write_all(&header)?;
    writer.write_all(&json_bytes)?;
    if !frame.binary.is_empty() {
        writer.write_all(&frame.binary)?;
    }
    writer.flush()
}

/// Reads one framed message from `reader`. Returns `Ok(None)` on clean EOF.
pub fn read_frame<R: Read>(reader: &mut R) -> io::Result<Option<Frame>> {
    let mut len_buf = [0u8; 4];
    match reader.read_exact(&mut len_buf) {
        Ok(()) => {}
        Err(error) if error.kind() == io::ErrorKind::UnexpectedEof => return Ok(None),
        Err(error) => return Err(error),
    }
    let body_len = u32::from_le_bytes(len_buf) as usize;
    if !(13..=MAX_FRAME_BYTES).contains(&body_len) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("invalid frame body length {body_len}"),
        ));
    }
    let mut body = vec![0u8; body_len];
    reader.read_exact(&mut body)?;

    let kind = FrameKind::from_byte(body[0])?;
    let id = u64::from_le_bytes(body[1..9].try_into().expect("8-byte id"));
    let json_len = u32::from_le_bytes(body[9..13].try_into().expect("4-byte json length")) as usize;
    let json_end = 13usize
        .checked_add(json_len)
        .filter(|&end| end <= body_len)
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "frame json_len out of range"))?;
    let json: Value = serde_json::from_slice(&body[13..json_end])
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
    let binary = body[json_end..].to_vec();

    Ok(Some(Frame { kind, id, json, binary }))
}

/// Full-duplex client/server connection over a Unix stream with request/response correlation and an event callback.
pub struct ServiceConnection {
    writer: Mutex<UnixStream>,
    next_id: AtomicU64,
    pending: Arc<Mutex<HashMap<u64, mpsc::Sender<Frame>>>>,
}

impl ServiceConnection {
    /// Starts a reader thread on `stream` and delivers incoming requests and events to `on_incoming`.
    /// When the remote end disconnects, `on_disconnect` runs once.
    pub fn start(
        stream: UnixStream,
        thread_name: impl Into<String>,
        on_incoming: impl Fn(Frame) + Send + 'static,
        on_disconnect: impl FnOnce() + Send + 'static,
    ) -> io::Result<Arc<Self>> {
        let mut reader = stream.try_clone()?;
        let pending: Arc<Mutex<HashMap<u64, mpsc::Sender<Frame>>>> = Arc::new(Mutex::new(HashMap::new()));
        let conn = Arc::new(Self {
            writer: Mutex::new(stream),
            next_id: AtomicU64::new(1),
            pending: Arc::clone(&pending),
        });

        std::thread::Builder::new().name(thread_name.into()).spawn(move || {
            while let Ok(Some(frame)) = read_frame(&mut reader) {
                if frame.kind == FrameKind::Response {
                    let tx = pending.lock().unwrap_or_else(|e| e.into_inner()).remove(&frame.id);
                    if let Some(tx) = tx {
                        let _ = tx.send(frame);
                    }
                } else {
                    on_incoming(frame);
                }
            }
            // Wake any blocked sync callers immediately on disconnect.
            pending.lock().unwrap_or_else(|e| e.into_inner()).clear();
            on_disconnect();
        })?;

        Ok(conn)
    }

    /// Sends a one-way event or response frame.
    pub fn send(&self, frame: &Frame) -> io::Result<()> {
        let mut writer = self.writer.lock().unwrap_or_else(|e| e.into_inner());
        write_frame(&mut *writer, frame)
    }

    /// Sends a one-way event JSON payload.
    pub fn send_event(&self, json: Value) -> io::Result<()> {
        self.send(&Frame::event(json))
    }

    /// Sends a synchronous request and blocks until the matching response arrives or `timeout` elapses.
    pub fn request_sync(&self, json: Value, binary: Vec<u8>, timeout: Duration) -> io::Result<Frame> {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let (tx, rx) = mpsc::channel();
        self.pending.lock().unwrap_or_else(|e| e.into_inner()).insert(id, tx);

        if let Err(error) = self.send(&Frame::request_with_binary(id, json, binary)) {
            self.pending.lock().unwrap_or_else(|e| e.into_inner()).remove(&id);
            return Err(error);
        }

        match rx.recv_timeout(timeout) {
            Ok(frame) => Ok(frame),
            Err(mpsc::RecvTimeoutError::Timeout) => {
                self.pending.lock().unwrap_or_else(|e| e.into_inner()).remove(&id);
                Err(io::Error::new(io::ErrorKind::TimedOut, "service request timed out"))
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                Err(io::Error::new(io::ErrorKind::BrokenPipe, "service disconnected"))
            }
        }
    }

    /// Closes the underlying socket to trigger shutdown on both ends.
    pub fn shutdown(&self) {
        if let Ok(writer) = self.writer.lock() {
            let _ = writer.shutdown(std::net::Shutdown::Both);
        }
    }
}

/// Protocol version handshake payload exchanged when a service connects to the UI process.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HandshakeInfo {
    pub service: String,
    pub protocol_version: u32,
}

/// Runs the UI side of the version handshake on a newly spawned service connection.
/// Returns `Ok(info)` if versions match, or an error describing the mismatch so the caller can disable the service
/// without crashing the browser.
pub fn verify_service_handshake(
    conn: &ServiceConnection,
    expected_service: &str,
    expected_protocol_version: u32,
) -> io::Result<HandshakeInfo> {
    let reply = conn.request_sync(
        json!({
            "op": "hello",
            "expectedService": expected_service,
            "protocolVersion": expected_protocol_version,
        }),
        Vec::new(),
        Duration::from_secs(5),
    )?;

    let ok = reply.json.get("ok").and_then(Value::as_bool).unwrap_or(false);
    if !ok {
        let msg = reply
            .json
            .get("error")
            .and_then(Value::as_str)
            .unwrap_or("handshake rejected");
        return Err(io::Error::new(io::ErrorKind::PermissionDenied, msg));
    }

    let data = reply.json.get("data").unwrap_or(&Value::Null);
    let service = data
        .get("service")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    let protocol_version = data.get("protocolVersion").and_then(Value::as_u64).unwrap_or(0) as u32;

    if service != expected_service {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("unexpected service `{service}` (expected `{expected_service}`)"),
        ));
    }
    if protocol_version != expected_protocol_version {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!(
                "{expected_service} protocol version mismatch: service speaks v{protocol_version}, UI expects v{expected_protocol_version}"
            ),
        ));
    }

    Ok(HandshakeInfo {
        service,
        protocol_version,
    })
}

/// Takes ownership of a Unix socket passed from the parent UI process in `env_var`.
pub fn take_inherited_stream(env_var: &str) -> io::Result<UnixStream> {
    let raw = std::env::var(env_var).map_err(|_| {
        io::Error::new(
            io::ErrorKind::NotFound,
            format!("missing inherited socket environment variable `{env_var}`"),
        )
    })?;
    let fd: i32 = raw.parse().map_err(|_| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!("invalid file descriptor in `{env_var}`: `{raw}`"),
        )
    })?;
    if fd < 0 {
        return Err(io::Error::new(io::ErrorKind::InvalidData, "negative file descriptor"));
    }
    // SAFETY: the parent UI process cleared FD_CLOEXEC and passed exclusive ownership of `fd` to this child.
    let owned = unsafe { OwnedFd::from_raw_fd(fd) };
    Ok(UnixStream::from(owned))
}

/// Locates a Zendia service binary (such as `zendia-extension-server`) next to the current executable or via `env_var`.
pub fn find_service_executable(binary_name: &str, env_var: Option<&str>) -> io::Result<PathBuf> {
    if let Some(var) = env_var
        && let Some(path) = std::env::var_os(var)
    {
        return Ok(PathBuf::from(path));
    }

    let exe = std::env::current_exe()?;
    let mut candidates = Vec::new();
    if let Some(dir) = exe.parent() {
        candidates.push(dir.join(binary_name));
        // Support running from a test binary inside `target/<profile>/deps/`.
        if let Some(parent) = dir.parent() {
            candidates.push(parent.join(binary_name));
        }
    }
    for ancestor in exe.ancestors().skip(1) {
        candidates.push(ancestor.join("Zendia/target/release").join(binary_name));
        candidates.push(ancestor.join("Zendia/target/debug").join(binary_name));
    }

    candidates
        .into_iter()
        .find(|candidate| is_executable(candidate))
        .ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::NotFound,
                format!("unable to locate service executable `{binary_name}`"),
            )
        })
}

fn is_executable(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    path.metadata()
        .map(|metadata| metadata.is_file() && (metadata.permissions().mode() & 0o111) != 0)
        .unwrap_or(false)
}

/// A newly spawned Zendia service process together with its UI socket and optional Hub embedder socket.
pub struct SpawnedService {
    pub child: Child,
    pub ui_stream: UnixStream,
    pub embedder_hub_fd: Option<OwnedFd>,
}

/// Spawns a Zendia service binary with an inherited UI IPC socket and an optional direct Hub embedder socket.
pub fn spawn_service(executable: &Path, args: &[String], with_embedder_channel: bool) -> io::Result<SpawnedService> {
    let (ui_ours, ui_theirs) = UnixStream::pair()?;
    let ui_their_fd = ui_theirs.as_raw_fd();

    let (embedder_ours, embedder_theirs) = if with_embedder_channel {
        let (a, b) = UnixStream::pair()?;
        (Some(a), Some(b))
    } else {
        (None, None)
    };
    let embedder_their_fd = embedder_theirs.as_ref().map(AsRawFd::as_raw_fd);

    let mut command = Command::new(executable);
    command.args(args).env(ENV_UI_SOCKET_FD, ui_their_fd.to_string());
    if let Some(fd) = embedder_their_fd {
        command.env(ENV_EMBEDDER_SOCKET_FD, fd.to_string());
    }

    // SAFETY: pre_exec only invokes async-signal-safe fcntl() to clear FD_CLOEXEC on inherited socket fds.
    unsafe {
        command.pre_exec(move || {
            for fd in std::iter::once(ui_their_fd).chain(embedder_their_fd) {
                let flags = libc::fcntl(fd, libc::F_GETFD);
                if flags < 0 || libc::fcntl(fd, libc::F_SETFD, flags & !libc::FD_CLOEXEC) < 0 {
                    return Err(io::Error::last_os_error());
                }
            }
            Ok(())
        });
    }

    let child = command.spawn()?;
    drop(ui_theirs);
    drop(embedder_theirs);

    let embedder_hub_fd = embedder_ours.map(OwnedFd::from);
    Ok(SpawnedService {
        child,
        ui_stream: ui_ours,
        embedder_hub_fd,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frame_round_trip_with_json_and_binary() {
        let frame = Frame::request_with_binary(
            42,
            json!({ "op": "serve", "url": "chrome-extension://abc/icon.png" }),
            vec![1, 2, 3, 4, 255],
        );
        let mut buf = Vec::new();
        write_frame(&mut buf, &frame).unwrap();
        let mut cursor = io::Cursor::new(buf);
        let decoded = read_frame(&mut cursor).unwrap().unwrap();
        assert_eq!(decoded, frame);
        assert!(read_frame(&mut cursor).unwrap().is_none());
    }

    #[test]
    fn service_connection_sync_request_and_handshake() {
        let (a, b) = UnixStream::pair().unwrap();
        let server_conn = ServiceConnection::start(b, "test-service-server", |_| {}, || {}).unwrap();
        // Replace with an echoing server on a fresh pair.
        drop(server_conn);
        drop(a);

        let (client_sock, mut server_sock) = UnixStream::pair().unwrap();
        let server_thread = std::thread::spawn(move || {
            let req = read_frame(&mut server_sock).unwrap().unwrap();
            assert_eq!(req.op(), Some("hello"));
            let reply = Frame::response_ok(req.id, json!({ "service": "extension_server", "protocolVersion": 1 }));
            write_frame(&mut server_sock, &reply).unwrap();
        });

        let client = ServiceConnection::start(client_sock, "test-service-client", |_| {}, || {}).unwrap();
        let info = verify_service_handshake(&client, "extension_server", 1).unwrap();
        assert_eq!(info.service, "extension_server");
        assert_eq!(info.protocol_version, 1);
        server_thread.join().unwrap();
    }

    #[test]
    fn service_handshake_rejects_version_mismatch() {
        let (client_sock, mut server_sock) = UnixStream::pair().unwrap();
        let server_thread = std::thread::spawn(move || {
            let req = read_frame(&mut server_sock).unwrap().unwrap();
            let reply = Frame::response_ok(req.id, json!({ "service": "extension_server", "protocolVersion": 99 }));
            write_frame(&mut server_sock, &reply).unwrap();
        });

        let client = ServiceConnection::start(client_sock, "test-mismatch-client", |_| {}, || {}).unwrap();
        let err = verify_service_handshake(&client, "extension_server", 1).unwrap_err();
        assert!(err.to_string().contains("protocol version mismatch"));
        server_thread.join().unwrap();
    }
}
