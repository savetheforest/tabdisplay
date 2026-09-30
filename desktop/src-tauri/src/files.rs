//! Safe, bounded core for a future file-transfer capability.
//!
//! This module deliberately has no network, UI, or filesystem side effects. It
//! validates the transfer contract and hashes chunks incrementally so a later
//! transport can add acceptance/SAF/temp-file handling without loading an
//! entire user file into memory.

use sha2::{Digest, Sha256};
use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};

pub const MAX_FILE_BYTES: u64 = 4 * 1024 * 1024 * 1024;
pub const MAX_CHUNK_BYTES: usize = 64 * 1024;
pub const MAX_IN_FLIGHT_CHUNKS: usize = 4;
const MAX_NAME_BYTES: usize = 255;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TransferId([u8; 16]);

impl TransferId {
    pub fn from_bytes(bytes: [u8; 16]) -> Option<Self> {
        (bytes != [0; 16]).then_some(Self(bytes))
    }

    pub fn bytes(self) -> [u8; 16] {
        self.0
    }

    fn hex(self) -> String {
        self.0.iter().map(|byte| format!("{byte:02x}")).collect()
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum NameError {
    Empty,
    TooLong,
    PathSeparator,
    ControlCharacter,
    InvalidWindowsCharacter,
    TrailingSpaceOrDot,
    Traversal,
    ReservedWindowsName,
}

/// Validates a presentation name without silently turning an unsafe path into
/// another file. The future receiver must use this value as a name only, never
/// append an untrusted path supplied by the sender.
pub fn sanitize_name(name: &str) -> Result<String, NameError> {
    if name.is_empty() {
        return Err(NameError::Empty);
    }
    if name.as_bytes().len() > MAX_NAME_BYTES {
        return Err(NameError::TooLong);
    }
    if name == "." || name == ".." {
        return Err(NameError::Traversal);
    }
    if name.chars().any(char::is_control) {
        return Err(NameError::ControlCharacter);
    }
    if name.contains('/') || name.contains('\\') {
        return Err(NameError::PathSeparator);
    }
    if name
        .chars()
        .any(|c| matches!(c, '<' | '>' | ':' | '"' | '|' | '?' | '*'))
    {
        return Err(NameError::InvalidWindowsCharacter);
    }
    if name.ends_with(' ') || name.ends_with('.') {
        return Err(NameError::TrailingSpaceOrDot);
    }

    let stem = name.split('.').next().unwrap_or_default();
    let upper_stem = stem.to_ascii_uppercase();
    if matches!(upper_stem.as_str(), "CON" | "PRN" | "AUX" | "NUL")
        || (stem.len() == 4
            && (upper_stem.starts_with("COM") || upper_stem.starts_with("LPT"))
            && upper_stem.as_bytes()[3].is_ascii_digit()
            && upper_stem.as_bytes()[3] != b'0')
    {
        return Err(NameError::ReservedWindowsName);
    }

    Ok(name.to_owned())
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum MetaError {
    InvalidId,
    InvalidName(NameError),
    TooLarge,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TransferMeta {
    pub id: TransferId,
    pub name: String,
    pub size: u64,
    pub sha256: [u8; 32],
}

impl TransferMeta {
    pub fn new(id: [u8; 16], name: &str, size: u64, sha256: [u8; 32]) -> Result<Self, MetaError> {
        let id = TransferId::from_bytes(id).ok_or(MetaError::InvalidId)?;
        if size > MAX_FILE_BYTES {
            return Err(MetaError::TooLarge);
        }
        Ok(Self {
            id,
            name: sanitize_name(name).map_err(MetaError::InvalidName)?,
            size,
            sha256,
        })
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ChunkError {
    Cancelled,
    Empty,
    TooLarge,
    OffsetMismatch { expected: u64, received: u64 },
    ExceedsDeclaredSize,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum FinishError {
    Cancelled,
    Incomplete { received: u64, expected: u64 },
    HashMismatch,
}

/// Incremental receiver state. It retains only the hash state and counters;
/// callers own each bounded chunk and are responsible for the eventual
/// exclusive temporary destination.
pub struct ReceiverState {
    meta: TransferMeta,
    received: u64,
    hasher: Sha256,
    cancelled: bool,
}

impl ReceiverState {
    pub fn new(meta: TransferMeta) -> Self {
        Self {
            meta,
            received: 0,
            hasher: Sha256::new(),
            cancelled: false,
        }
    }

    pub fn received(&self) -> u64 {
        self.received
    }

    pub fn is_cancelled(&self) -> bool {
        self.cancelled
    }

    pub fn cancel(&mut self) {
        self.cancelled = true;
    }

    pub fn accept_chunk(&mut self, offset: u64, chunk: &[u8]) -> Result<(), ChunkError> {
        if self.cancelled {
            return Err(ChunkError::Cancelled);
        }
        if chunk.is_empty() {
            return Err(ChunkError::Empty);
        }
        if chunk.len() > MAX_CHUNK_BYTES {
            return Err(ChunkError::TooLarge);
        }
        if offset != self.received {
            return Err(ChunkError::OffsetMismatch {
                expected: self.received,
                received: offset,
            });
        }
        let next = self
            .received
            .checked_add(chunk.len() as u64)
            .ok_or(ChunkError::ExceedsDeclaredSize)?;
        if next > self.meta.size {
            return Err(ChunkError::ExceedsDeclaredSize);
        }
        self.hasher.update(chunk);
        self.received = next;
        Ok(())
    }

    pub fn finish(&self) -> Result<(), FinishError> {
        if self.cancelled {
            return Err(FinishError::Cancelled);
        }
        if self.received != self.meta.size {
            return Err(FinishError::Incomplete {
                received: self.received,
                expected: self.meta.size,
            });
        }
        if self.hasher.clone().finalize().as_slice() != self.meta.sha256 {
            return Err(FinishError::HashMismatch);
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DestinationError {
    MissingDirectory,
    DirectoryNotDirectory,
    DirectoryReparsePoint,
    FinalExists,
    FinalIsReparsePoint,
    TempCollision,
    Io(io::ErrorKind),
    Chunk(ChunkError),
    Finish(FinishError),
}

impl From<io::Error> for DestinationError {
    fn from(error: io::Error) -> Self {
        Self::Io(error.kind())
    }
}

fn is_reparse_point(metadata: &fs::Metadata) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x400;
        metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
    }
    #[cfg(not(windows))]
    {
        metadata.file_type().is_symlink()
    }
}

/// A receiver destination selected by the user. It never follows a symlink or
/// reparse-point destination itself and never opens the final file for write.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReceiveDestination {
    directory: PathBuf,
}

impl ReceiveDestination {
    pub fn prepare(directory: impl AsRef<Path>) -> Result<Self, DestinationError> {
        let directory = directory.as_ref();
        let metadata = fs::symlink_metadata(directory).map_err(|error| {
            if error.kind() == io::ErrorKind::NotFound {
                DestinationError::MissingDirectory
            } else {
                error.into()
            }
        })?;
        if is_reparse_point(&metadata) {
            return Err(DestinationError::DirectoryReparsePoint);
        }
        if !metadata.is_dir() {
            return Err(DestinationError::DirectoryNotDirectory);
        }
        Ok(Self {
            directory: directory.to_path_buf(),
        })
    }

    pub fn directory(&self) -> &Path {
        &self.directory
    }

    pub fn begin(&self, meta: TransferMeta) -> Result<PendingFile, DestinationError> {
        let final_path = self.directory.join(&meta.name);
        match fs::symlink_metadata(&final_path) {
            Ok(metadata) if is_reparse_point(&metadata) => {
                return Err(DestinationError::FinalIsReparsePoint)
            }
            Ok(metadata) if metadata.is_dir() => return Err(DestinationError::FinalExists),
            Ok(_) => return Err(DestinationError::FinalExists),
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }

        let prefix = format!(".tabdisplay-transfer-{}", meta.id.hex());
        for attempt in 0..8u8 {
            let suffix = if attempt == 0 {
                String::new()
            } else {
                format!("-{attempt}")
            };
            let temp_path = self.directory.join(format!("{prefix}{suffix}.part"));
            match OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&temp_path)
            {
                Ok(file) => {
                    return Ok(PendingFile {
                        meta: meta.clone(),
                        state: ReceiverState::new(meta.clone()),
                        final_path,
                        temp_path,
                        file: Some(file),
                        committed: false,
                    });
                }
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
                Err(error) => return Err(error.into()),
            }
        }
        Err(DestinationError::TempCollision)
    }
}

/// Bounded file receiver. The caller supplies one chunk at a time; the file
/// is never loaded into memory and the final destination is untouched until a
/// successful hash/size check.
pub struct PendingFile {
    meta: TransferMeta,
    state: ReceiverState,
    final_path: PathBuf,
    temp_path: PathBuf,
    file: Option<File>,
    committed: bool,
}

impl PendingFile {
    pub fn final_path(&self) -> &Path {
        &self.final_path
    }

    pub fn temp_path(&self) -> &Path {
        &self.temp_path
    }

    pub fn received(&self) -> u64 {
        self.state.received()
    }

    pub fn accept_chunk(&mut self, offset: u64, chunk: &[u8]) -> Result<(), DestinationError> {
        self.state
            .accept_chunk(offset, chunk)
            .map_err(DestinationError::Chunk)?;
        self.file
            .as_mut()
            .ok_or(DestinationError::Io(io::ErrorKind::InvalidInput))?
            .write_all(chunk)
            .map_err(Into::into)
    }

    pub fn finish(mut self) -> Result<PathBuf, DestinationError> {
        self.state.finish().map_err(DestinationError::Finish)?;
        if let Some(file) = self.file.take() {
            file.sync_all().map_err(DestinationError::from)?;
        }
        match fs::symlink_metadata(&self.final_path) {
            Ok(metadata) if is_reparse_point(&metadata) => {
                return Err(DestinationError::FinalIsReparsePoint)
            }
            Ok(_) => return Err(DestinationError::FinalExists),
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
        match fs::rename(&self.temp_path, &self.final_path) {
            Ok(()) => {
                self.committed = true;
                Ok(self.final_path.clone())
            }
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {
                Err(DestinationError::FinalExists)
            }
            Err(error) => Err(error.into()),
        }
    }

    pub fn cancel(mut self) -> Result<(), DestinationError> {
        self.file.take();
        match fs::remove_file(&self.temp_path) {
            Ok(()) => {
                self.committed = true;
                Ok(())
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                self.committed = true;
                Ok(())
            }
            Err(error) => Err(error.into()),
        }
    }

    pub fn metadata(&self) -> &TransferMeta {
        &self.meta
    }
}

impl Drop for PendingFile {
    fn drop(&mut self) {
        if !self.committed {
            self.file.take();
            let _ = fs::remove_file(&self.temp_path);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn meta(name: &str, contents: &[u8]) -> TransferMeta {
        let mut hash = Sha256::new();
        hash.update(contents);
        TransferMeta::new([1; 16], name, contents.len() as u64, hash.finalize().into()).unwrap()
    }

    #[test]
    fn rejects_path_traversal_reserved_and_invalid_names() {
        for name in [
            "",
            ".",
            "..",
            "../video.mp4",
            "C:\\video.mp4",
            "CON.txt",
            "report?.txt",
            "name.",
        ] {
            assert!(
                sanitize_name(name).is_err(),
                "accepted unsafe name {name:?}"
            );
        }
        assert_eq!(sanitize_name("foto ção.txt").unwrap(), "foto ção.txt");
    }

    #[test]
    fn validates_id_size_and_name_limits() {
        assert!(TransferId::from_bytes([0; 16]).is_none());
        assert_eq!(
            TransferMeta::new([1; 16], "x", MAX_FILE_BYTES + 1, [0; 32]),
            Err(MetaError::TooLarge)
        );
        assert_eq!(
            TransferMeta::new([1; 16], &"x".repeat(MAX_NAME_BYTES + 1), 0, [0; 32]),
            Err(MetaError::InvalidName(NameError::TooLong))
        );
    }

    #[test]
    fn accepts_zero_byte_file_with_correct_hash() {
        assert!(ReceiverState::new(meta("empty.bin", &[])).finish().is_ok());
    }

    #[test]
    fn hashes_bounded_sequential_chunks_and_rejects_gaps() {
        let contents = b"abcdef";
        let mut receiver = ReceiverState::new(meta("data.bin", contents));
        assert_eq!(
            receiver.accept_chunk(1, b"a"),
            Err(ChunkError::OffsetMismatch {
                expected: 0,
                received: 1
            })
        );
        receiver.accept_chunk(0, b"abc").unwrap();
        receiver.accept_chunk(3, b"def").unwrap();
        assert_eq!(receiver.received(), contents.len() as u64);
        assert!(receiver.finish().is_ok());
    }

    #[test]
    fn rejects_oversized_chunk_cancel_and_bad_hash() {
        let mut receiver = ReceiverState::new(meta("data.bin", b"abc"));
        assert_eq!(
            receiver.accept_chunk(0, &vec![0; MAX_CHUNK_BYTES + 1]),
            Err(ChunkError::TooLarge)
        );
        receiver.cancel();
        assert_eq!(receiver.accept_chunk(0, b"abc"), Err(ChunkError::Cancelled));
        assert_eq!(receiver.finish(), Err(FinishError::Cancelled));

        let mut bad =
            ReceiverState::new(TransferMeta::new([2; 16], "bad.bin", 3, [0; 32]).unwrap());
        bad.accept_chunk(0, b"abc").unwrap();
        assert_eq!(bad.finish(), Err(FinishError::HashMismatch));
    }

    #[test]
    fn does_not_accept_more_than_declared_size() {
        let mut receiver = ReceiverState::new(meta("short.bin", b"a"));
        assert_eq!(
            receiver.accept_chunk(0, b"ab"),
            Err(ChunkError::ExceedsDeclaredSize)
        );
        assert_eq!(
            receiver.finish(),
            Err(FinishError::Incomplete {
                received: 0,
                expected: 1
            })
        );
    }

    fn test_directory() -> PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!("tabdisplay-file-test-{nonce}"));
        fs::create_dir(&path).unwrap();
        path
    }

    #[test]
    fn receives_chunks_without_loading_them_and_commits_verified_file() {
        let dir = test_directory();
        let contents = b"bounded file payload";
        let destination = ReceiveDestination::prepare(&dir).unwrap();
        let mut pending = destination.begin(meta("payload.bin", contents)).unwrap();
        let temp_path = pending.temp_path().to_path_buf();
        pending.accept_chunk(0, &contents[..7]).unwrap();
        pending.accept_chunk(7, &contents[7..]).unwrap();
        let final_path = pending.finish().unwrap();
        assert_eq!(fs::read(&final_path).unwrap(), contents);
        assert!(!temp_path.exists());
        fs::remove_file(final_path).unwrap();
        fs::remove_dir(dir).unwrap();
    }

    #[test]
    fn bad_hash_and_cancel_remove_only_the_owned_temp_file() {
        let dir = test_directory();
        let destination = ReceiveDestination::prepare(&dir).unwrap();
        let mut bad = destination
            .begin(TransferMeta::new([4; 16], "bad.bin", 3, [0; 32]).unwrap())
            .unwrap();
        let bad_temp = bad.temp_path().to_path_buf();
        bad.accept_chunk(0, b"abc").unwrap();
        assert_eq!(
            bad.finish(),
            Err(DestinationError::Finish(FinishError::HashMismatch))
        );
        assert!(!bad_temp.exists());

        let mut cancelled = destination.begin(meta("cancel.bin", b"abc")).unwrap();
        let cancelled_temp = cancelled.temp_path().to_path_buf();
        cancelled.accept_chunk(0, b"abc").unwrap();
        cancelled.cancel().unwrap();
        assert!(!cancelled_temp.exists());
        fs::remove_dir(dir).unwrap();
    }

    #[test]
    fn existing_destination_wins_race_and_is_preserved() {
        let dir = test_directory();
        let destination = ReceiveDestination::prepare(&dir).unwrap();
        let mut pending = destination.begin(meta("race.bin", b"new")).unwrap();
        let temp_path = pending.temp_path().to_path_buf();
        fs::write(dir.join("race.bin"), b"old").unwrap();
        pending.accept_chunk(0, b"new").unwrap();
        assert_eq!(pending.finish(), Err(DestinationError::FinalExists));
        assert_eq!(fs::read(dir.join("race.bin")).unwrap(), b"old");
        assert!(!temp_path.exists());
        fs::remove_file(dir.join("race.bin")).unwrap();
        fs::remove_dir(dir).unwrap();
    }

    #[test]
    fn destination_requires_an_existing_real_directory() {
        let dir = test_directory();
        assert_eq!(
            ReceiveDestination::prepare(dir.join("missing")),
            Err(DestinationError::MissingDirectory)
        );
        let file = dir.join("not-a-directory");
        fs::write(&file, b"x").unwrap();
        assert_eq!(
            ReceiveDestination::prepare(&file),
            Err(DestinationError::DirectoryNotDirectory)
        );
        fs::remove_file(file).unwrap();
        fs::remove_dir(dir).unwrap();
    }
}
