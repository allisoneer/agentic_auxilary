use crate::state::InterruptionKind;
use anyhow::Context;
use anyhow::Result;
use serde::Deserialize;
use serde::Serialize;
use sha2::Digest;
use sha2::Sha256;
use std::fs::DirBuilder;
use std::fs::OpenOptions;
use std::io::Write as _;
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::DirBuilderExt;
use std::os::unix::fs::FileTypeExt;
use std::os::unix::fs::MetadataExt;
use std::os::unix::fs::OpenOptionsExt;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::path::PathBuf;
use thoughts_tool::utils::locks::FileLock;
use tokio::io::AsyncRead;
use tokio::io::AsyncReadExt;
use tokio::io::AsyncWrite;
use tokio::io::AsyncWriteExt;
use tokio::net::UnixListener;
use tokio::net::UnixStream;

const PROTOCOL_VERSION: u32 = 1;
const MAX_FRAME_BYTES: usize = 64 * 1024;
const MAX_RUNTIME_FILE_BYTES: u64 = MAX_FRAME_BYTES as u64;
const MAX_SOCKET_PATH_BYTES: usize = 100;
#[cfg(not(test))]
const IPC_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(5);
#[cfg(test)]
const IPC_TIMEOUT: std::time::Duration = std::time::Duration::from_millis(500);

pub struct OwnerRuntime {
    _lock: FileLock,
    listener: UnixListener,
    paths: RuntimePaths,
    owner_token: String,
    worktree_hash: String,
}

pub struct OwnerMutationLease {
    _lock: FileLock,
}

#[derive(Debug, Clone)]
struct RuntimePaths {
    lock: PathBuf,
    socket: PathBuf,
    manifest: PathBuf,
    manifest_temp: PathBuf,
    secret: PathBuf,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct InterruptionCorrelation {
    pub run_id: String,
    pub invocation_id: String,
    pub session_id: String,
    pub command_message_id: String,
    pub kind: InterruptionKind,
    pub request_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum InterruptionResponse {
    Permission { allow: bool },
    Question { answers: Vec<Vec<String>> },
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct OwnerManifest {
    protocol_version: u32,
    worktree_hash: String,
    socket_path: PathBuf,
    pending: InterruptionCorrelation,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct IpcRequest {
    protocol_version: u32,
    worktree_hash: String,
    owner_token: String,
    correlation: InterruptionCorrelation,
    response: InterruptionResponse,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct IpcReply {
    accepted: bool,
    error: Option<String>,
}

impl OwnerRuntime {
    pub fn acquire(worktree: &Path) -> Result<Self> {
        let canonical = worktree
            .canonicalize()
            .with_context(|| format!("failed to canonicalize worktree {}", worktree.display()))?;
        let worktree_hash = worktree_hash(&canonical);
        let paths = runtime_paths(&worktree_hash)?;
        let lock = FileLock::try_lock_exclusive(&paths.lock)?.ok_or_else(|| {
            anyhow::anyhow!("another agentic-outer-dag owner is active for this worktree")
        })?;
        std::fs::set_permissions(&paths.lock, std::fs::Permissions::from_mode(0o600))?;
        validate_private_regular_file(&paths.lock, current_uid())?;

        remove_stale_runtime_file(&paths.socket)?;
        remove_stale_runtime_file(&paths.manifest)?;
        remove_stale_runtime_file(&paths.manifest_temp)?;
        remove_stale_runtime_file(&paths.secret)?;
        let listener = UnixListener::bind(&paths.socket)
            .with_context(|| format!("failed to bind owner socket {}", paths.socket.display()))?;
        std::fs::set_permissions(&paths.socket, std::fs::Permissions::from_mode(0o600))?;
        validate_private_socket(&paths.socket, current_uid())?;
        let owner_token = generate_owner_token()?;
        write_private_file(&paths.secret, owner_token.as_bytes(), true)?;

        Ok(Self {
            _lock: lock,
            listener,
            paths,
            owner_token,
            worktree_hash,
        })
    }

    pub fn publish_pending(&self, correlation: &InterruptionCorrelation) -> Result<()> {
        let manifest = OwnerManifest {
            protocol_version: PROTOCOL_VERSION,
            worktree_hash: self.worktree_hash.clone(),
            socket_path: self.paths.socket.clone(),
            pending: correlation.clone(),
        };
        let bytes = serde_json::to_vec(&manifest)?;
        write_private_file_atomic(&self.paths.manifest_temp, &self.paths.manifest, &bytes)
    }

    pub fn clear_pending(&self) -> Result<()> {
        remove_stale_runtime_file(&self.paths.manifest)
    }

    pub async fn await_response(
        &self,
        expected: &InterruptionCorrelation,
    ) -> Result<InterruptionResponse> {
        loop {
            let (mut stream, _) = self.listener.accept().await?;
            let request: IpcRequest = match read_frame_bounded(&mut stream).await {
                Ok(request) => request,
                Err(error) => {
                    reject_reply(&mut stream, error.to_string()).await;
                    continue;
                }
            };
            let validation = self.validate_request(&request, expected);
            if let Err(error) = validation {
                reject_reply(&mut stream, error.to_string()).await;
                continue;
            }
            write_reply_bounded(&mut stream, true, None).await?;
            self.clear_pending()?;
            return Ok(request.response);
        }
    }

    fn validate_request(
        &self,
        request: &IpcRequest,
        expected: &InterruptionCorrelation,
    ) -> Result<()> {
        anyhow::ensure!(
            request.protocol_version == PROTOCOL_VERSION,
            "IPC protocol mismatch"
        );
        anyhow::ensure!(
            request.worktree_hash == self.worktree_hash,
            "worktree mismatch"
        );
        anyhow::ensure!(
            constant_time_eq(request.owner_token.as_bytes(), self.owner_token.as_bytes()),
            "owner authentication failed"
        );
        anyhow::ensure!(
            &request.correlation == expected,
            "interruption correlation mismatch"
        );
        anyhow::ensure!(
            response_kind(&request.response) == expected.kind,
            "response kind mismatch"
        );
        Ok(())
    }
}

impl OwnerMutationLease {
    pub fn acquire(worktree: &Path) -> Result<Self> {
        let canonical = worktree.canonicalize()?;
        let paths = runtime_paths(&worktree_hash(&canonical))?;
        let lock = FileLock::try_lock_exclusive(&paths.lock)?.ok_or_else(|| {
            anyhow::anyhow!("another agentic-outer-dag owner is active for this worktree")
        })?;
        Ok(Self { _lock: lock })
    }
}

impl Drop for OwnerRuntime {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.paths.socket);
        let _ = std::fs::remove_file(&self.paths.manifest);
        let _ = std::fs::remove_file(&self.paths.manifest_temp);
        let _ = std::fs::remove_file(&self.paths.secret);
    }
}

pub async fn send_response(worktree: &Path, response: InterruptionResponse) -> Result<()> {
    let canonical = worktree.canonicalize()?;
    let hash = worktree_hash(&canonical);
    let paths = runtime_paths(&hash)?;
    let uid = current_uid();
    validate_private_regular_file(&paths.manifest, uid).with_context(|| {
        "no live foreground owner is awaiting this response; keep persisted pending state and recover conservatively"
    })?;
    validate_private_regular_file(&paths.secret, uid)?;
    validate_private_socket(&paths.socket, uid)?;
    let manifest_bytes = std::fs::read(&paths.manifest)?;
    let manifest: OwnerManifest = serde_json::from_slice(&manifest_bytes)?;
    anyhow::ensure!(
        manifest.protocol_version == PROTOCOL_VERSION,
        "IPC protocol mismatch"
    );
    anyhow::ensure!(manifest.worktree_hash == hash, "worktree mismatch");
    anyhow::ensure!(
        manifest.socket_path == paths.socket,
        "unsafe owner socket path"
    );
    let owner_token = std::fs::read_to_string(&paths.secret)?;
    let request = IpcRequest {
        protocol_version: PROTOCOL_VERSION,
        worktree_hash: hash,
        owner_token,
        correlation: manifest.pending,
        response,
    };
    let mut stream = tokio::time::timeout(IPC_TIMEOUT, UnixStream::connect(&paths.socket))
        .await
        .context("timed out connecting to foreground owner")?
        .with_context(
            || "owner socket is unavailable; keep persisted pending state and recover conservatively",
        )?;
    tokio::time::timeout(IPC_TIMEOUT, write_frame(&mut stream, &request))
        .await
        .context("timed out sending response to foreground owner")??;
    let reply: IpcReply = tokio::time::timeout(IPC_TIMEOUT, read_frame(&mut stream))
        .await
        .context("timed out awaiting foreground owner acknowledgement")??;
    anyhow::ensure!(
        reply.accepted,
        "owner rejected response: {}",
        reply.error.unwrap_or_default()
    );
    Ok(())
}

fn response_kind(response: &InterruptionResponse) -> InterruptionKind {
    match response {
        InterruptionResponse::Permission { .. } => InterruptionKind::Permission,
        InterruptionResponse::Question { .. } => InterruptionKind::Question,
    }
}

async fn reject_reply<W: AsyncWrite + Unpin>(stream: &mut W, error: String) {
    let _ = write_reply_bounded(stream, false, Some(error)).await;
}

async fn write_reply_bounded<W: AsyncWrite + Unpin>(
    stream: &mut W,
    accepted: bool,
    error: Option<String>,
) -> Result<()> {
    tokio::time::timeout(IPC_TIMEOUT, write_reply(stream, accepted, error))
        .await
        .context("timed out writing owner IPC reply")??;
    Ok(())
}

async fn write_reply<W: AsyncWrite + Unpin>(
    stream: &mut W,
    accepted: bool,
    error: Option<String>,
) -> Result<()> {
    write_frame(stream, &IpcReply { accepted, error }).await
}

async fn write_frame<T: Serialize + Sync, W: AsyncWrite + Unpin>(
    stream: &mut W,
    value: &T,
) -> Result<()> {
    let bytes = serde_json::to_vec(value)?;
    anyhow::ensure!(
        bytes.len() <= MAX_FRAME_BYTES,
        "IPC frame exceeds maximum size"
    );
    stream.write_u32(bytes.len().try_into()?).await?;
    stream.write_all(&bytes).await?;
    stream.flush().await?;
    Ok(())
}

async fn read_frame_bounded<T: for<'de> Deserialize<'de>, R: AsyncRead + Unpin>(
    stream: &mut R,
) -> Result<T> {
    tokio::time::timeout(IPC_TIMEOUT, read_frame(stream))
        .await
        .context("timed out reading accepted owner IPC connection")?
}

async fn read_frame<T: for<'de> Deserialize<'de>, R: AsyncRead + Unpin>(
    stream: &mut R,
) -> Result<T> {
    let length = stream.read_u32().await? as usize;
    anyhow::ensure!(length <= MAX_FRAME_BYTES, "IPC frame exceeds maximum size");
    let mut bytes = vec![0; length];
    stream.read_exact(&mut bytes).await?;
    Ok(serde_json::from_slice(&bytes)?)
}

fn runtime_paths(worktree_hash: &str) -> Result<RuntimePaths> {
    let uid = current_uid();
    let preferred_base = match std::env::var_os("XDG_RUNTIME_DIR").map(PathBuf::from) {
        Some(path) if validate_private_directory(&path, uid).is_ok() => path,
        _ => short_runtime_fallback(uid),
    };
    let preferred = build_runtime_paths(&preferred_base, worktree_hash);
    let base = if preferred.socket.as_os_str().as_bytes().len() < MAX_SOCKET_PATH_BYTES {
        preferred_base
    } else {
        short_runtime_fallback(uid)
    };
    create_private_directory(&base, uid)?;
    let app = base.join("agentic-outer-dag");
    create_private_directory(&app, uid)?;
    let directory = app.join(worktree_hash);
    create_private_directory(&directory, uid)?;
    let paths = RuntimePaths {
        lock: directory.join("owner.lock"),
        socket: directory.join("owner.sock"),
        manifest: directory.join("owner.json"),
        manifest_temp: directory.join("owner.tmp"),
        secret: directory.join("owner.secret"),
    };
    anyhow::ensure!(
        paths.socket.as_os_str().as_bytes().len() < MAX_SOCKET_PATH_BYTES,
        "owner socket path is too long"
    );
    Ok(paths)
}

fn build_runtime_paths(base: &Path, worktree_hash: &str) -> RuntimePaths {
    let directory = base.join("agentic-outer-dag").join(worktree_hash);
    RuntimePaths {
        lock: directory.join("owner.lock"),
        socket: directory.join("owner.sock"),
        manifest: directory.join("owner.json"),
        manifest_temp: directory.join("owner.tmp"),
        secret: directory.join("owner.secret"),
    }
}

fn short_runtime_fallback(uid: u32) -> PathBuf {
    std::env::temp_dir().join(format!("agentic-outer-dag-{uid}"))
}

fn current_uid() -> u32 {
    rustix::process::geteuid().as_raw()
}

fn create_private_directory(path: &Path, uid: u32) -> Result<()> {
    DirBuilder::new().recursive(true).mode(0o700).create(path)?;
    validate_private_directory(path, uid)
}

fn validate_private_directory(path: &Path, uid: u32) -> Result<()> {
    let metadata = std::fs::symlink_metadata(path)?;
    anyhow::ensure!(
        metadata.file_type().is_dir(),
        "runtime path is not a directory"
    );
    anyhow::ensure!(
        !metadata.file_type().is_symlink(),
        "runtime directory is a symlink"
    );
    anyhow::ensure!(metadata.uid() == uid, "runtime directory owner mismatch");
    anyhow::ensure!(
        metadata.permissions().mode().trailing_zeros() >= 6,
        "runtime directory is not private"
    );
    Ok(())
}

fn validate_private_regular_file(path: &Path, uid: u32) -> Result<()> {
    let metadata = std::fs::symlink_metadata(path)?;
    anyhow::ensure!(
        metadata.file_type().is_file(),
        "runtime artifact is not a regular file"
    );
    anyhow::ensure!(
        !metadata.file_type().is_symlink(),
        "runtime artifact is a symlink"
    );
    anyhow::ensure!(metadata.uid() == uid, "runtime artifact owner mismatch");
    anyhow::ensure!(
        metadata.permissions().mode().trailing_zeros() >= 6,
        "runtime artifact is not private"
    );
    anyhow::ensure!(
        metadata.len() <= MAX_RUNTIME_FILE_BYTES,
        "runtime artifact exceeds maximum size"
    );
    Ok(())
}

fn validate_private_socket(path: &Path, uid: u32) -> Result<()> {
    let metadata = std::fs::symlink_metadata(path)?;
    anyhow::ensure!(
        metadata.file_type().is_socket(),
        "runtime socket is not a socket"
    );
    anyhow::ensure!(
        !metadata.file_type().is_symlink(),
        "runtime socket is a symlink"
    );
    anyhow::ensure!(metadata.uid() == uid, "runtime socket owner mismatch");
    anyhow::ensure!(
        metadata.permissions().mode().trailing_zeros() >= 6,
        "runtime socket is not private"
    );
    Ok(())
}

fn remove_stale_runtime_file(path: &Path) -> Result<()> {
    match std::fs::symlink_metadata(path) {
        Ok(metadata) => {
            anyhow::ensure!(
                !metadata.file_type().is_symlink(),
                "refusing to remove runtime symlink"
            );
            anyhow::ensure!(
                metadata.file_type().is_file() || metadata.file_type().is_socket(),
                "refusing to remove unexpected runtime artifact"
            );
            std::fs::remove_file(path)?;
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(error.into()),
    }
    Ok(())
}

fn write_private_file(path: &Path, bytes: &[u8], create_new: bool) -> Result<()> {
    anyhow::ensure!(
        bytes.len() as u64 <= MAX_RUNTIME_FILE_BYTES,
        "runtime artifact exceeds maximum size"
    );
    let uid = current_uid();
    match std::fs::symlink_metadata(path) {
        Ok(_) => validate_private_regular_file(path, uid)?,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(error.into()),
    }
    let mut options = OpenOptions::new();
    options.write(true).mode(0o600);
    if create_new {
        options.create_new(true);
    } else {
        options.create(true).truncate(true);
    }
    let mut file = options.open(path)?;
    file.write_all(bytes)?;
    file.sync_all()?;
    validate_private_regular_file(path, uid)
}

fn write_private_file_atomic(temp: &Path, destination: &Path, bytes: &[u8]) -> Result<()> {
    remove_stale_runtime_file(temp)?;
    if destination.exists() {
        validate_private_regular_file(destination, current_uid())?;
    }
    write_private_file(temp, bytes, true)?;
    std::fs::rename(temp, destination)?;
    validate_private_regular_file(destination, current_uid())
}

fn generate_owner_token() -> Result<String> {
    let mut bytes = [0_u8; 32];
    getrandom::fill(&mut bytes)
        .map_err(|error| anyhow::anyhow!("secure owner-token generation failed: {error}"))?;
    Ok(hex_encode(&bytes))
}

fn worktree_hash(worktree: &Path) -> String {
    let digest = Sha256::digest(worktree.as_os_str().as_bytes());
    hex_encode(&digest[..8])
}

fn hex_encode(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        output.push(char::from(HEX[usize::from(byte >> 4)]));
        output.push(char::from(HEX[usize::from(byte & 0x0f)]));
    }
    output
}

fn constant_time_eq(left: &[u8], right: &[u8]) -> bool {
    if left.len() != right.len() {
        return false;
    }
    left.iter()
        .zip(right)
        .fold(0_u8, |difference, (left, right)| {
            difference | (left ^ right)
        })
        == 0
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::EnvVarGuard;
    use crate::test_support::process_state_lock;
    use tempfile::TempDir;

    fn permission_correlation() -> InterruptionCorrelation {
        InterruptionCorrelation {
            run_id: "run-1".to_string(),
            invocation_id: "inv-1".to_string(),
            session_id: "session-1".to_string(),
            command_message_id: "msg-1".to_string(),
            kind: InterruptionKind::Permission,
            request_id: "permission-1".to_string(),
        }
    }

    #[tokio::test]
    async fn owner_lock_is_exclusive_and_releases_on_drop() {
        let worktree = TempDir::new().unwrap();
        let owner = OwnerRuntime::acquire(worktree.path()).unwrap();
        assert!(OwnerRuntime::acquire(worktree.path()).is_err());
        drop(owner);
        assert!(OwnerRuntime::acquire(worktree.path()).is_ok());
    }

    #[tokio::test]
    async fn mutation_lease_blocks_owner_until_released() {
        let worktree = TempDir::new().unwrap();
        let lease = OwnerMutationLease::acquire(worktree.path()).unwrap();

        assert!(OwnerRuntime::acquire(worktree.path()).is_err());
        drop(lease);
        assert!(OwnerRuntime::acquire(worktree.path()).is_ok());
    }

    #[tokio::test]
    async fn active_owner_blocks_mutation_lease_until_released() {
        let worktree = TempDir::new().unwrap();
        let owner = OwnerRuntime::acquire(worktree.path()).unwrap();

        assert!(OwnerMutationLease::acquire(worktree.path()).is_err());
        drop(owner);
        assert!(OwnerMutationLease::acquire(worktree.path()).is_ok());
    }

    #[tokio::test]
    async fn authenticated_response_roundtrip_is_single_use() {
        let worktree = TempDir::new().unwrap();
        let owner = OwnerRuntime::acquire(worktree.path()).unwrap();
        let correlation = InterruptionCorrelation {
            run_id: "run-1".to_string(),
            invocation_id: "inv-1".to_string(),
            session_id: "session-1".to_string(),
            command_message_id: "msg-1".to_string(),
            kind: InterruptionKind::Permission,
            request_id: "permission-1".to_string(),
        };
        owner.publish_pending(&correlation).unwrap();
        let send = send_response(
            worktree.path(),
            InterruptionResponse::Permission { allow: true },
        );
        let receive = owner.await_response(&correlation);
        let (send_result, received) = tokio::join!(send, receive);
        send_result.unwrap();
        assert_eq!(
            received.unwrap(),
            InterruptionResponse::Permission { allow: true }
        );
        assert!(
            send_response(
                worktree.path(),
                InterruptionResponse::Permission { allow: true }
            )
            .await
            .is_err()
        );
    }

    #[tokio::test]
    async fn stalled_length_prefix_times_out_then_valid_responder_succeeds() {
        let worktree = TempDir::new().unwrap();
        let owner = OwnerRuntime::acquire(worktree.path()).unwrap();
        let correlation = permission_correlation();
        owner.publish_pending(&correlation).unwrap();

        let receive = owner.await_response(&correlation);
        let send_after_stall = async {
            let _stalled = UnixStream::connect(&owner.paths.socket).await.unwrap();
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
            send_response(
                worktree.path(),
                InterruptionResponse::Permission { allow: true },
            )
            .await
        };
        let (received, sent) = tokio::time::timeout(std::time::Duration::from_secs(2), async {
            tokio::join!(receive, send_after_stall)
        })
        .await
        .unwrap();

        sent.unwrap();
        assert_eq!(
            received.unwrap(),
            InterruptionResponse::Permission { allow: true }
        );
    }

    #[tokio::test]
    async fn partial_frame_body_times_out_then_valid_responder_succeeds() {
        let worktree = TempDir::new().unwrap();
        let owner = OwnerRuntime::acquire(worktree.path()).unwrap();
        let correlation = permission_correlation();
        owner.publish_pending(&correlation).unwrap();

        let receive = owner.await_response(&correlation);
        let send_after_partial_frame = async {
            let mut stalled = UnixStream::connect(&owner.paths.socket).await.unwrap();
            stalled.write_u32(32).await.unwrap();
            stalled.write_all(b"{").await.unwrap();
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
            send_response(
                worktree.path(),
                InterruptionResponse::Permission { allow: false },
            )
            .await
        };
        let (received, sent) = tokio::time::timeout(std::time::Duration::from_secs(2), async {
            tokio::join!(receive, send_after_partial_frame)
        })
        .await
        .unwrap();

        sent.unwrap();
        assert_eq!(
            received.unwrap(),
            InterruptionResponse::Permission { allow: false }
        );
    }

    #[tokio::test]
    async fn malformed_rejection_reply_write_failure_is_nonfatal() {
        let (mut writer, reader) = tokio::io::duplex(1);
        drop(reader);

        reject_reply(&mut writer, "malformed request".to_string()).await;
    }

    #[tokio::test]
    async fn validation_rejection_reply_write_failure_is_nonfatal() {
        let (mut writer, reader) = tokio::io::duplex(1);
        drop(reader);

        reject_reply(&mut writer, "owner authentication failed".to_string()).await;
    }

    #[tokio::test]
    async fn valid_acceptance_reply_write_timeout_is_fatal() {
        let (mut writer, _reader) = tokio::io::duplex(1);

        let error = write_reply_bounded(&mut writer, true, None)
            .await
            .expect_err("valid acceptance acknowledgement timeout must fail");

        assert!(
            error
                .to_string()
                .contains("timed out writing owner IPC reply")
        );
    }

    #[tokio::test]
    async fn owner_rejects_token_correlation_and_kind_mismatches() {
        let worktree = TempDir::new().unwrap();
        let owner = OwnerRuntime::acquire(worktree.path()).unwrap();
        let expected = InterruptionCorrelation {
            run_id: "run-1".to_string(),
            invocation_id: "inv-1".to_string(),
            session_id: "session-1".to_string(),
            command_message_id: "msg-1".to_string(),
            kind: InterruptionKind::Permission,
            request_id: "permission-1".to_string(),
        };
        let mut request = IpcRequest {
            protocol_version: PROTOCOL_VERSION,
            worktree_hash: owner.worktree_hash.clone(),
            owner_token: "wrong".to_string(),
            correlation: expected.clone(),
            response: InterruptionResponse::Permission { allow: true },
        };
        assert!(owner.validate_request(&request, &expected).is_err());
        request.owner_token = owner.owner_token.clone();
        request.protocol_version = PROTOCOL_VERSION + 1;
        assert!(owner.validate_request(&request, &expected).is_err());
        request.protocol_version = PROTOCOL_VERSION;
        request.worktree_hash = "wrong-worktree".to_string();
        assert!(owner.validate_request(&request, &expected).is_err());
        request.worktree_hash = owner.worktree_hash.clone();
        request.correlation.request_id = "wrong-request".to_string();
        assert!(owner.validate_request(&request, &expected).is_err());
        for correlation in [
            InterruptionCorrelation {
                run_id: "wrong-run".to_string(),
                ..expected.clone()
            },
            InterruptionCorrelation {
                invocation_id: "wrong-invocation".to_string(),
                ..expected.clone()
            },
            InterruptionCorrelation {
                session_id: "wrong-session".to_string(),
                ..expected.clone()
            },
            InterruptionCorrelation {
                command_message_id: "wrong-message".to_string(),
                ..expected.clone()
            },
        ] {
            request.correlation = correlation.clone();
            assert!(owner.validate_request(&request, &expected).is_err());
        }
        request.correlation = expected.clone();
        request.response = InterruptionResponse::Question {
            answers: vec![vec!["yes".to_string()]],
        };
        assert!(owner.validate_request(&request, &expected).is_err());
    }

    #[tokio::test]
    async fn oversized_frame_is_rejected_before_allocation() {
        let (mut writer, mut reader) = UnixStream::pair().unwrap();
        writer
            .write_u32((MAX_FRAME_BYTES + 1).try_into().unwrap())
            .await
            .unwrap();
        let result = read_frame::<IpcReply, _>(&mut reader).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn malformed_frame_is_rejected() {
        let (mut writer, mut reader) = UnixStream::pair().unwrap();
        writer.write_u32(1).await.unwrap();
        writer.write_all(b"{").await.unwrap();
        let result = read_frame::<IpcReply, _>(&mut reader).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn runtime_artifacts_are_private_and_socket_path_is_short() {
        let worktree = TempDir::new().unwrap();
        let owner = OwnerRuntime::acquire(worktree.path()).unwrap();
        let correlation = InterruptionCorrelation {
            run_id: "run-1".to_string(),
            invocation_id: "inv-1".to_string(),
            session_id: "session-1".to_string(),
            command_message_id: "msg-1".to_string(),
            kind: InterruptionKind::Question,
            request_id: "question-1".to_string(),
        };
        owner.publish_pending(&correlation).unwrap();
        assert_eq!(
            std::fs::metadata(owner.paths.socket.parent().unwrap())
                .unwrap()
                .permissions()
                .mode()
                & 0o077,
            0
        );
        assert!(owner.paths.socket.as_os_str().len() < 100);
        assert_eq!(
            std::fs::metadata(&owner.paths.secret)
                .unwrap()
                .permissions()
                .mode()
                & 0o077,
            0
        );
        assert_eq!(
            std::fs::metadata(&owner.paths.manifest)
                .unwrap()
                .permissions()
                .mode()
                & 0o077,
            0
        );
        assert_eq!(
            std::fs::metadata(&owner.paths.socket)
                .unwrap()
                .permissions()
                .mode()
                & 0o077,
            0
        );
    }

    #[tokio::test]
    async fn owner_acquisition_is_independent_of_home() {
        let _guard = process_state_lock().lock().unwrap();
        let runtime = TempDir::new().unwrap();
        std::fs::set_permissions(runtime.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
        let _home = EnvVarGuard::remove("HOME");
        let _runtime_dir = EnvVarGuard::set("XDG_RUNTIME_DIR", runtime.path());
        let worktree = TempDir::new().unwrap();

        let owner = OwnerRuntime::acquire(worktree.path()).unwrap();

        assert_eq!(
            std::fs::metadata(&owner.paths.socket).unwrap().uid(),
            rustix::process::geteuid().as_raw()
        );
    }

    #[test]
    fn worktree_hash_is_stable_for_the_same_canonical_path() {
        let worktree = TempDir::new().unwrap();
        let canonical = worktree.path().canonicalize().unwrap();
        assert_eq!(worktree_hash(&canonical), worktree_hash(&canonical));
        assert_eq!(worktree_hash(&canonical).len(), 16);
    }

    #[test]
    fn stale_cleanup_rejects_symlinks_and_unexpected_file_types() {
        let temp = TempDir::new().unwrap();
        let target = temp.path().join("target");
        std::fs::write(&target, "target").unwrap();
        let link = temp.path().join("link");
        std::os::unix::fs::symlink(&target, &link).unwrap();
        assert!(remove_stale_runtime_file(&link).is_err());

        let directory = temp.path().join("directory");
        std::fs::create_dir_all(&directory).unwrap();
        assert!(remove_stale_runtime_file(&directory).is_err());
    }

    #[tokio::test]
    async fn missing_owner_returns_conservative_guidance() {
        let worktree = TempDir::new().unwrap();
        let error = send_response(
            worktree.path(),
            InterruptionResponse::Permission { allow: true },
        )
        .await
        .unwrap_err();
        assert!(error.to_string().contains("no live foreground owner"));
    }

    #[tokio::test]
    async fn racing_responders_accept_exactly_one_response() {
        let worktree = TempDir::new().unwrap();
        let owner = OwnerRuntime::acquire(worktree.path()).unwrap();
        let correlation = InterruptionCorrelation {
            run_id: "run-1".to_string(),
            invocation_id: "inv-1".to_string(),
            session_id: "session-1".to_string(),
            command_message_id: "msg-1".to_string(),
            kind: InterruptionKind::Permission,
            request_id: "permission-1".to_string(),
        };
        owner.publish_pending(&correlation).unwrap();

        let first = send_response(
            worktree.path(),
            InterruptionResponse::Permission { allow: true },
        );
        let second = send_response(
            worktree.path(),
            InterruptionResponse::Permission { allow: false },
        );
        let receive = owner.await_response(&correlation);
        let (first, second, received) = tokio::join!(first, second, receive);

        assert!(received.is_ok());
        assert_eq!(usize::from(first.is_ok()) + usize::from(second.is_ok()), 1);
    }

    #[tokio::test]
    async fn owner_acquisition_removes_stale_socket_artifact_while_locked() {
        let worktree = TempDir::new().unwrap();
        let canonical = worktree.path().canonicalize().unwrap();
        let paths = runtime_paths(&worktree_hash(&canonical)).unwrap();
        std::fs::write(&paths.socket, "stale").unwrap();

        let owner = OwnerRuntime::acquire(worktree.path()).unwrap();

        assert!(
            std::fs::symlink_metadata(&owner.paths.socket)
                .unwrap()
                .file_type()
                .is_socket()
        );
    }

    #[tokio::test]
    #[expect(
        clippy::await_holding_lock,
        reason = "test intentionally serializes process-wide environment mutation"
    )]
    async fn responder_does_not_require_a_valid_opencode_binary() {
        let _guard = process_state_lock().lock().unwrap();
        let _binary = EnvVarGuard::set("OPENCODE_BINARY", "/does/not/exist");
        let worktree = TempDir::new().unwrap();
        let owner = OwnerRuntime::acquire(worktree.path()).unwrap();
        let correlation = InterruptionCorrelation {
            run_id: "run-1".to_string(),
            invocation_id: "inv-1".to_string(),
            session_id: "session-1".to_string(),
            command_message_id: "msg-1".to_string(),
            kind: InterruptionKind::Permission,
            request_id: "permission-1".to_string(),
        };
        owner.publish_pending(&correlation).unwrap();

        let send = send_response(
            worktree.path(),
            InterruptionResponse::Permission { allow: true },
        );
        let receive = owner.await_response(&correlation);
        let (send_result, received) = tokio::join!(send, receive);

        assert!(send_result.is_ok());
        assert!(received.is_ok());
    }
}
