/*
 * Copyright (c) 2026-present, the Ladybird developers.
 *
 * SPDX-License-Identifier: BSD-2-Clause
 */

//! The UI's end of its connection to the Hub, in Rust: the Hub's messages and the types they carry, starting the Hub,
//! and the connection that sends to it and hands on what it tells the UI.

pub mod ipc_types;
pub mod location;

use libipc_rust::Attachment;
use libipc_rust::transport::TransportSocket;
use libipc_rust::wire::{self, Decoder, EndpointMessage, SyncMessage};
use std::io;
use std::os::fd::AsRawFd;
use std::os::unix::net::UnixStream;
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex};
use std::time::Duration;

pub use ipc_types::hub_client::Message as HubMessage;
pub use ipc_types::*;

/// What the reader thread hands to the UI.
pub enum Incoming {
    Message(Box<HubMessage>),
    /// The Hub closed the connection, or sent something it must not. Either way, it is gone for the UI.
    Disconnected,
}

#[derive(Debug)]
pub enum Error {
    Io(io::Error),
    Encode(wire::EncodeError),
    Decode(wire::DecodeError),
    Disconnected,
    UnexpectedResponse { expected: i32, got: i32 },
    HubNotFound,
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(error) => write!(f, "{error}"),
            Self::Encode(error) => write!(f, "could not encode a message: {error:?}"),
            Self::Decode(error) => write!(f, "could not decode a message: {error:?}"),
            Self::Disconnected => write!(f, "the Hub is gone"),
            Self::UnexpectedResponse { expected, got } => {
                write!(f, "the Hub answered with message {got}, not {expected}")
            }
            Self::HubNotFound => write!(f, "unable to find the Hub"),
        }
    }
}

impl std::error::Error for Error {}

impl From<io::Error> for Error {
    fn from(error: io::Error) -> Self {
        Self::Io(error)
    }
}

impl From<wire::EncodeError> for Error {
    fn from(error: wire::EncodeError) -> Self {
        Self::Encode(error)
    }
}

impl From<wire::DecodeError> for Error {
    fn from(error: wire::DecodeError) -> Self {
        Self::Decode(error)
    }
}

type RawMessage = (Vec<u8>, Vec<Attachment>);

/// How long the UI waits for the Hub to answer a synchronous message before it takes the Hub to be stuck.
const SYNC_RESPONSE_TIMEOUT: Duration = Duration::from_secs(30);

pub struct HubConnection {
    transport: Arc<TransportSocket>,
    // NB: The UI sends synchronous messages from one thread, one at a time, so one answer at most is ever awaited.
    responses: Mutex<Receiver<RawMessage>>,
}

impl HubConnection {
    /// Starts reading what the Hub sends on a thread of its own. Each message the UI handles goes to `on_message`,
    /// which must hand it to the UI thread, and the answers to synchronous messages go to whoever awaits them.
    pub fn new(socket: UnixStream, on_message: impl Fn(Incoming) + Send + 'static) -> Result<Arc<Self>, Error> {
        let transport = Arc::new(TransportSocket::new(socket.into())?);
        let (response_sender, responses) = mpsc::channel();
        let connection = Arc::new(Self {
            transport: Arc::clone(&transport),
            responses: Mutex::new(responses),
        });
        std::thread::Builder::new()
            .name("Hub reader".to_string())
            .spawn(move || read_messages(&transport, &response_sender, &on_message))?;
        Ok(connection)
    }

    pub fn post<M: EndpointMessage>(&self, message: M) -> Result<(), Error> {
        let (bytes, attachments) = message.encode_message()?;
        self.transport.post_message(bytes, attachments).map_err(|error| {
            if error.kind() == io::ErrorKind::NotConnected {
                Error::Disconnected
            } else {
                Error::Io(error)
            }
        })
    }

    /// Sends a message and logs why it could not, for the many messages whose sender has nothing else to do then.
    pub fn post_or_log<M: EndpointMessage>(&self, message: M) {
        if let Err(error) = self.post(message) {
            eprintln!("zendia: Unable to send message {} to the Hub: {error}", M::MESSAGE_ID);
        }
    }

    pub fn send_sync<M: SyncMessage>(&self, message: M) -> Result<M::Response, Error> {
        let responses = self.responses.lock().unwrap_or_else(|error| error.into_inner());
        // NB: An answer that came after its sender stopped waiting is no answer to this message.
        while responses.try_recv().is_ok() {}
        self.post(message)?;
        let (bytes, attachments) = match responses.recv_timeout(SYNC_RESPONSE_TIMEOUT) {
            Ok(response) => response,
            Err(RecvTimeoutError::Timeout | RecvTimeoutError::Disconnected) => return Err(Error::Disconnected),
        };
        let mut decoder = Decoder::new(&bytes, attachments);
        let (_, id) = wire::decode_message_header(&mut decoder)?;
        let expected = <M::Response as EndpointMessage>::MESSAGE_ID;
        if id != expected {
            return Err(Error::UnexpectedResponse { expected, got: id });
        }
        Ok(decoder.decode()?)
    }
}

fn read_messages(transport: &TransportSocket, responses: &Sender<RawMessage>, on_message: &impl Fn(Incoming)) {
    while let Some(received) = transport.receive() {
        let (bytes, attachments) = received.into_parts();
        let bytes: Vec<u8> = AsRef::<[u8]>::as_ref(&bytes).to_vec();
        let magic = match wire::decode_message_header(&mut Decoder::new(&bytes, Vec::new())) {
            Ok((magic, _)) => magic,
            Err(error) => {
                eprintln!("zendia: The Hub sent a message without a header: {error:?}");
                break;
            }
        };
        if magic == ipc_types::hub_server::ENDPOINT_MAGIC {
            let _ = responses.send((bytes, attachments));
            continue;
        }
        match HubMessage::decode(&bytes, attachments) {
            Ok(Some(message)) => on_message(Incoming::Message(Box::new(message))),
            // NB: The Hub tells the UI more than this UI does anything with yet.
            Ok(None) => {}
            Err(error) => {
                let id = wire::decode_message_header(&mut Decoder::new(&bytes, Vec::new()))
                    .map(|(_, id)| id)
                    .unwrap_or_default();
                eprintln!("zendia: Unable to decode message {id} from the Hub: {error:?}");
                break;
            }
        }
    }
    on_message(Incoming::Disconnected);
}

/// Finds the Hub among the helper processes of the build or installation this executable belongs to.
pub fn find_hub_executable() -> Result<PathBuf, Error> {
    if let Some(path) = std::env::var_os("LADYBIRD_HUB") {
        return Ok(PathBuf::from(path));
    }
    let executable = std::env::current_exe()?;
    let mut candidates = Vec::new();
    for directory in executable.ancestors().skip(1) {
        candidates.push(directory.join("libexec/Hub"));
        candidates.push(directory.join("Build/release/libexec/Hub"));
    }
    candidates
        .into_iter()
        .find(|candidate| is_executable(candidate))
        .ok_or(Error::HubNotFound)
}

fn is_executable(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    path.metadata()
        .map(|metadata| metadata.is_file() && metadata.permissions().mode() & 0o111 != 0)
        .unwrap_or(false)
}

fn encode_hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

/// Starts the Hub, which takes over the socket it is given as its connection to the UI.
pub fn spawn_hub(hub: &Path, arguments: &[String], ui_info: Hub_UIInfo) -> Result<(Child, UnixStream), Error> {
    let mut encoder = wire::Encoder::new();
    encoder.encode_owned(ui_info)?;
    let (ui_info_bytes, _) = encoder.finish();

    let (ours, theirs) = UnixStream::pair()?;
    let their_fd = theirs.as_raw_fd();

    let mut command = Command::new(hub);
    command
        .args(arguments)
        .arg(format!("--ui-info={}", encode_hex(&ui_info_bytes)))
        .arg(format!("--ui-pid={}", std::process::id()))
        .env("SOCKET_TAKEOVER", format!("ipc:{their_fd}"));
    // SAFETY: Between fork and exec, this only calls fcntl(), which is async-signal-safe.
    unsafe {
        command.pre_exec(move || {
            // NB: The Hub's end must outlive the exec, so it is the one descriptor that loses its close-on-exec flag.
            let flags = libc::fcntl(their_fd, libc::F_GETFD);
            if flags < 0 || libc::fcntl(their_fd, libc::F_SETFD, flags & !libc::FD_CLOEXEC) < 0 {
                return Err(io::Error::last_os_error());
            }
            Ok(())
        });
    }
    let child = command.spawn()?;
    drop(theirs);
    Ok((child, ours))
}
