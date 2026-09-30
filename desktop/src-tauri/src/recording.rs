//! Bounded recording contract for a future explicit PC-side writer.
//!
//! The selected container is Matroska: it can carry the existing H.264
//! access units and timestamped Opus packets without pretending that Annex-B
//! and Opus bytes are one valid MP4 stream. This module owns no files and
//! never blocks the streaming loop; a later writer drains its bounded queue.

use std::collections::VecDeque;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, SyncSender, TrySendError};
use std::thread::{self, JoinHandle};

static TEMP_SEQUENCE: AtomicU64 = AtomicU64::new(0);

pub const MAX_QUEUE_BYTES: usize = 16 << 20;
pub const MAX_PACKET_BYTES: usize = 4 << 20;
pub const MAX_DURATION_MS: u64 = 2 * 60 * 60 * 1000;
/// A writer owns a small handoff queue. Disk I/O must never be performed by
/// the capture/streaming loop, and an already full queue is reported instead
/// of growing without a bound.
pub const WRITER_QUEUE_SEGMENTS: usize = 2;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Format {
    MatroskaH264Opus,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StreamConfig {
    pub generation: u64,
    pub width: u32,
    pub height: u32,
    pub fps: u32,
    pub audio: bool,
}

impl StreamConfig {
    pub(crate) fn valid(&self) -> bool {
        (16..=7680).contains(&self.width)
            && (16..=7680).contains(&self.height)
            && self.width % 16 == 0
            && self.height % 16 == 0
            && (1..=240).contains(&self.fps)
            && (self.width as u64) * (self.height as u64) <= 16_777_216
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Packet {
    Video {
        pts_ms: u64,
        keyframe: bool,
        data: Vec<u8>,
    },
    Audio {
        pts_samples: u64,
        data: Vec<u8>,
    },
}

impl Packet {
    fn len(&self) -> usize {
        match self {
            Self::Video { data, .. } | Self::Audio { data, .. } => data.len(),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Status {
    Idle,
    WaitingForKeyframe,
    Recording,
    SegmentRequired,
    Paused,
    Cancelled,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ConfigOutcome {
    Started,
    Unchanged,
    SegmentRequired,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PushOutcome {
    Accepted,
    DroppedUntilKeyframe,
    DroppedMutedAudio,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RecordError {
    InvalidConfig,
    Inactive,
    Paused,
    SegmentRequired,
    PacketTooLarge,
    QueueFull,
    NonMonotonicTimestamp,
    DurationLimit,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DestinationError {
    EmptyPath,
    MissingFileName,
    ParentMissing,
    ParentNotDirectory,
    FinalExists,
    FinalIsDirectory,
    TempCollision,
    Io(io::ErrorKind),
}

impl From<io::Error> for DestinationError {
    fn from(error: io::Error) -> Self {
        Self::Io(error.kind())
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum WriterError {
    Mux(crate::matroska::MuxError),
    Destination(DestinationError),
    QueueFull,
    Closed,
    WorkerPanic,
}

impl From<DestinationError> for WriterError {
    fn from(error: DestinationError) -> Self {
        Self::Destination(error)
    }
}

struct SegmentJob {
    destination: Destination,
    config: StreamConfig,
    packets: Vec<Packet>,
}

/// Asynchronous bounded handoff for complete recording segments.
///
/// `submit` uses `try_send`, so the caller never waits for disk I/O. Each job
/// gets its own destination and temporary file; a resize/codec-generation
/// change can therefore rotate to another final path without concatenating
/// unrelated Matroska segments into one file.
pub struct SegmentWriter {
    tx: Option<SyncSender<SegmentJob>>,
    done: Option<JoinHandle<Result<Vec<PathBuf>, WriterError>>>,
}

impl SegmentWriter {
    pub fn new() -> Self {
        let (tx, rx) = mpsc::sync_channel(WRITER_QUEUE_SEGMENTS);
        let done = thread::spawn(move || write_segments(rx));
        Self {
            tx: Some(tx),
            done: Some(done),
        }
    }

    pub fn submit(
        &self,
        destination: Destination,
        config: StreamConfig,
        packets: Vec<Packet>,
    ) -> Result<(), WriterError> {
        let tx = self.tx.as_ref().ok_or(WriterError::Closed)?;
        tx.try_send(SegmentJob {
            destination,
            config,
            packets,
        })
        .map_err(|error| match error {
            TrySendError::Full(_) => WriterError::QueueFull,
            TrySendError::Disconnected(_) => WriterError::Closed,
        })
    }

    /// Stops accepting jobs and waits for the worker to commit every accepted
    /// segment. A worker error is returned and later jobs are not claimed as
    /// committed.
    pub fn finish(mut self) -> Result<Vec<PathBuf>, WriterError> {
        self.tx.take();
        let done = self.done.take().ok_or(WriterError::Closed)?;
        done.join().map_err(|_| WriterError::WorkerPanic)?
    }
}

impl Drop for SegmentWriter {
    fn drop(&mut self) {
        self.tx.take();
        if let Some(done) = self.done.take() {
            let _ = done.join();
        }
    }
}

fn write_segments(rx: Receiver<SegmentJob>) -> Result<Vec<PathBuf>, WriterError> {
    let mut committed = Vec::new();
    for job in rx {
        let mut temp = job
            .destination
            .create_temp()
            .map_err(WriterError::Destination)?;
        temp.write_matroska_segment(&job.config, &job.packets)?;
        let path = temp.commit().map_err(WriterError::Destination)?;
        committed.push(path);
    }
    Ok(committed)
}

/// Validates a user-selected final path without creating or replacing it.
///
/// The temporary file is always created in the same parent directory so that
/// the later commit can use a same-volume rename. A caller must still call
/// `TempRecording::commit` or `TempRecording::cancel`; neither operation can
/// touch an unrelated path.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Destination {
    final_path: PathBuf,
}

impl Destination {
    pub fn prepare(path: impl AsRef<Path>) -> Result<Self, DestinationError> {
        let path = path.as_ref();
        if path.as_os_str().is_empty() {
            return Err(DestinationError::EmptyPath);
        }
        if path.file_name().is_none() {
            return Err(DestinationError::MissingFileName);
        }
        match fs::symlink_metadata(path) {
            Ok(metadata) if metadata.is_dir() => return Err(DestinationError::FinalIsDirectory),
            Ok(_) => return Err(DestinationError::FinalExists),
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
        let parent = path.parent().unwrap_or_else(|| Path::new("."));
        if !parent.exists() {
            return Err(DestinationError::ParentMissing);
        }
        if !parent.is_dir() {
            return Err(DestinationError::ParentNotDirectory);
        }
        Ok(Self {
            final_path: path.to_path_buf(),
        })
    }

    pub fn final_path(&self) -> &Path {
        &self.final_path
    }

    /// Derives a deterministic sibling path for a later codec/resize segment.
    /// Segment zero keeps the user-selected name; later segments add a numeric
    /// suffix before the extension and are still checked by `prepare`.
    pub fn rotated_path(base: &Path, segment: u32) -> PathBuf {
        if segment == 0 {
            return base.to_path_buf();
        }
        let stem = base
            .file_stem()
            .and_then(|value| value.to_str())
            .unwrap_or("recording");
        let extension = base.extension().and_then(|value| value.to_str());
        let name = match extension {
            Some(extension) if !extension.is_empty() => {
                format!("{stem}.{segment:03}.{extension}")
            }
            _ => format!("{stem}.{segment:03}"),
        };
        base.parent().unwrap_or_else(|| Path::new(".")).join(name)
    }

    fn temp_path(&self, sequence: u64) -> PathBuf {
        let parent = self.final_path.parent().unwrap_or_else(|| Path::new("."));
        parent.join(format!(
            ".tabdisplay-recording-{}-{}.part",
            std::process::id(),
            sequence
        ))
    }

    /// Creates an exclusive, identified temporary file. Existing files are
    /// never opened or truncated, even if a generated name collides.
    pub fn create_temp(&self) -> Result<TempRecording, DestinationError> {
        for _ in 0..8 {
            let sequence = TEMP_SEQUENCE
                .fetch_add(1, Ordering::Relaxed)
                .wrapping_add(1);
            let temp_path = self.temp_path(sequence);
            match OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&temp_path)
            {
                Ok(file) => {
                    return Ok(TempRecording {
                        destination: self.clone(),
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

/// Owns exactly one temporary recording path until commit or cancellation.
/// This is file lifecycle only; it does not claim to write a valid container.
pub struct TempRecording {
    destination: Destination,
    temp_path: PathBuf,
    file: Option<File>,
    committed: bool,
}

impl TempRecording {
    pub fn temp_path(&self) -> &Path {
        &self.temp_path
    }

    pub fn final_path(&self) -> &Path {
        self.destination.final_path()
    }

    /// Test/integration hook for a future muxer. It only writes bytes supplied
    /// by that muxer and never performs an unbounded buffering operation.
    pub fn write_bytes(&mut self, bytes: &[u8]) -> Result<(), DestinationError> {
        self.file
            .as_mut()
            .ok_or(DestinationError::Io(io::ErrorKind::InvalidInput))?
            .write_all(bytes)
            .map_err(Into::into)
    }

    /// Writes one independently structured Matroska segment into this
    /// transfer-owned temporary. The session still decides when to call it;
    /// this method never starts recording by itself.
    pub fn write_matroska_segment(
        &mut self,
        config: &StreamConfig,
        packets: &[Packet],
    ) -> Result<(), WriterError> {
        let bytes = crate::matroska::encode_segment(config, packets).map_err(WriterError::Mux)?;
        self.write_bytes(&bytes).map_err(WriterError::Destination)
    }

    pub fn commit(mut self) -> Result<PathBuf, DestinationError> {
        match fs::symlink_metadata(self.destination.final_path()) {
            Ok(metadata) if metadata.is_dir() => return Err(DestinationError::FinalIsDirectory),
            Ok(_) => return Err(DestinationError::FinalExists),
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
        if let Some(file) = self.file.take() {
            file.sync_all().map_err(DestinationError::from)?;
        }
        match fs::rename(&self.temp_path, self.destination.final_path()) {
            Ok(()) => {
                self.committed = true;
                Ok(self.destination.final_path().to_path_buf())
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
}

impl Drop for TempRecording {
    fn drop(&mut self) {
        if !self.committed {
            self.file.take();
            let _ = fs::remove_file(&self.temp_path);
        }
    }
}

/// State machine for a bounded writer queue. `drain_segment` is the handoff
/// point where a future Matroska writer can perform disk I/O on its own thread.
pub struct Recorder {
    format: Format,
    config: Option<StreamConfig>,
    pending_config: Option<StreamConfig>,
    queue: VecDeque<Packet>,
    queue_bytes: usize,
    status: Status,
    paused: bool,
    audio_enabled: bool,
    first_video_pts_ms: Option<u64>,
    last_video_pts_ms: Option<u64>,
    last_audio_pts_samples: Option<u64>,
}

impl Recorder {
    pub fn new(format: Format) -> Self {
        Self {
            format,
            config: None,
            pending_config: None,
            queue: VecDeque::new(),
            queue_bytes: 0,
            status: Status::Idle,
            paused: false,
            audio_enabled: true,
            first_video_pts_ms: None,
            last_video_pts_ms: None,
            last_audio_pts_samples: None,
        }
    }

    pub fn format(&self) -> Format {
        self.format
    }

    pub fn status(&self) -> Status {
        if self.paused && self.status != Status::Cancelled {
            Status::Paused
        } else {
            self.status
        }
    }

    pub fn queued_bytes(&self) -> usize {
        self.queue_bytes
    }

    pub fn config(&self) -> Option<StreamConfig> {
        self.config.clone()
    }

    pub fn set_config(&mut self, config: StreamConfig) -> Result<ConfigOutcome, RecordError> {
        if !config.valid() {
            return Err(RecordError::InvalidConfig);
        }
        match &self.config {
            None => {
                self.audio_enabled = config.audio;
                self.config = Some(config);
                self.status = Status::WaitingForKeyframe;
                Ok(ConfigOutcome::Started)
            }
            Some(current) if current == &config => Ok(ConfigOutcome::Unchanged),
            Some(_) => {
                self.pending_config = Some(config);
                self.status = Status::SegmentRequired;
                Ok(ConfigOutcome::SegmentRequired)
            }
        }
    }

    pub fn set_paused(&mut self, paused: bool) {
        self.paused = paused;
        if !paused && self.status == Status::Paused {
            self.status = if self.first_video_pts_ms.is_some() {
                Status::Recording
            } else {
                Status::WaitingForKeyframe
            };
        }
    }

    pub fn set_audio_enabled(&mut self, enabled: bool) {
        self.audio_enabled = enabled;
    }

    pub fn push_video(
        &mut self,
        pts_ms: u64,
        keyframe: bool,
        data: Vec<u8>,
    ) -> Result<PushOutcome, RecordError> {
        if self.config.is_none() {
            return Err(RecordError::Inactive);
        }
        if self.paused {
            return Err(RecordError::Paused);
        }
        if self.pending_config.is_some() {
            return Err(RecordError::SegmentRequired);
        }
        if data.is_empty() || data.len() > MAX_PACKET_BYTES {
            return Err(RecordError::PacketTooLarge);
        }
        if self.last_video_pts_ms.is_some_and(|last| pts_ms < last) {
            return Err(RecordError::NonMonotonicTimestamp);
        }
        if self
            .first_video_pts_ms
            .is_some_and(|first| pts_ms.saturating_sub(first) > MAX_DURATION_MS)
        {
            return Err(RecordError::DurationLimit);
        }
        self.last_video_pts_ms = Some(pts_ms);
        if !self.first_video_pts_ms.is_some() && !keyframe {
            return Ok(PushOutcome::DroppedUntilKeyframe);
        }
        if self.first_video_pts_ms.is_none() {
            self.first_video_pts_ms = Some(pts_ms);
        }
        self.enqueue(Packet::Video {
            pts_ms,
            keyframe,
            data,
        })?;
        self.status = Status::Recording;
        Ok(PushOutcome::Accepted)
    }

    pub fn push_audio(
        &mut self,
        pts_samples: u64,
        data: Vec<u8>,
    ) -> Result<PushOutcome, RecordError> {
        if self.config.is_none() {
            return Err(RecordError::Inactive);
        }
        if self.paused {
            return Err(RecordError::Paused);
        }
        if self.pending_config.is_some() {
            return Err(RecordError::SegmentRequired);
        }
        if !self.audio_enabled {
            return Ok(PushOutcome::DroppedMutedAudio);
        }
        if self.first_video_pts_ms.is_none() {
            return Ok(PushOutcome::DroppedUntilKeyframe);
        }
        if data.is_empty() || data.len() > MAX_PACKET_BYTES {
            return Err(RecordError::PacketTooLarge);
        }
        if self
            .last_audio_pts_samples
            .is_some_and(|last| pts_samples < last)
        {
            return Err(RecordError::NonMonotonicTimestamp);
        }
        self.last_audio_pts_samples = Some(pts_samples);
        self.enqueue(Packet::Audio { pts_samples, data })?;
        Ok(PushOutcome::Accepted)
    }

    fn enqueue(&mut self, packet: Packet) -> Result<(), RecordError> {
        let size = packet.len();
        if self.queue_bytes.saturating_add(size) > MAX_QUEUE_BYTES {
            return Err(RecordError::QueueFull);
        }
        self.queue_bytes += size;
        self.queue.push_back(packet);
        Ok(())
    }

    /// Hands the current segment to the writer and starts the next one at its
    /// next IDR/configuration. Existing packets are never deleted implicitly.
    pub fn drain_segment(&mut self) -> Vec<Packet> {
        let segment = self.queue.drain(..).collect();
        self.queue_bytes = 0;
        self.first_video_pts_ms = None;
        self.last_video_pts_ms = None;
        self.last_audio_pts_samples = None;
        if let Some(config) = self.pending_config.take() {
            self.audio_enabled = config.audio;
            self.config = Some(config);
        }
        self.status = if self.config.is_some() {
            Status::WaitingForKeyframe
        } else {
            Status::Idle
        };
        segment
    }

    pub fn cancel(&mut self) {
        self.queue.clear();
        self.queue_bytes = 0;
        self.pending_config = None;
        self.config = None;
        self.status = Status::Cancelled;
        self.first_video_pts_ms = None;
        self.last_video_pts_ms = None;
        self.last_audio_pts_samples = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn config(generation: u64) -> StreamConfig {
        StreamConfig {
            generation,
            width: 1152,
            height: 640,
            fps: 60,
            audio: true,
        }
    }

    #[test]
    fn waits_for_idr_then_keeps_monotonic_video_and_audio() {
        let mut recorder = Recorder::new(Format::MatroskaH264Opus);
        assert_eq!(recorder.format(), Format::MatroskaH264Opus);
        assert_eq!(recorder.set_config(config(1)), Ok(ConfigOutcome::Started));
        assert_eq!(
            recorder.push_video(0, false, vec![1]),
            Ok(PushOutcome::DroppedUntilKeyframe)
        );
        assert_eq!(
            recorder.push_audio(0, vec![1]),
            Ok(PushOutcome::DroppedUntilKeyframe)
        );
        assert_eq!(
            recorder.push_video(33, true, vec![2]),
            Ok(PushOutcome::Accepted)
        );
        assert_eq!(recorder.push_audio(960, vec![3]), Ok(PushOutcome::Accepted));
        assert_eq!(
            recorder.push_video(20, false, vec![4]),
            Err(RecordError::NonMonotonicTimestamp)
        );
        assert_eq!(recorder.status(), Status::Recording);
        assert_eq!(recorder.drain_segment().len(), 2);
        assert_eq!(recorder.status(), Status::WaitingForKeyframe);
    }

    #[test]
    fn resize_requires_a_new_segment_and_idr() {
        let mut recorder = Recorder::new(Format::MatroskaH264Opus);
        recorder.set_config(config(1)).unwrap();
        recorder.push_video(0, true, vec![1]).unwrap();
        assert_eq!(
            recorder.set_config(config(2)),
            Ok(ConfigOutcome::SegmentRequired)
        );
        assert_eq!(
            recorder.push_video(33, true, vec![2]),
            Err(RecordError::SegmentRequired)
        );
        assert_eq!(recorder.drain_segment().len(), 1);
        assert_eq!(
            recorder.push_video(33, false, vec![3]),
            Ok(PushOutcome::DroppedUntilKeyframe)
        );
        assert_eq!(
            recorder.push_video(66, true, vec![4]),
            Ok(PushOutcome::Accepted)
        );
    }

    #[test]
    fn pause_mute_and_cancel_do_not_touch_a_drained_segment() {
        let mut recorder = Recorder::new(Format::MatroskaH264Opus);
        recorder.set_config(config(1)).unwrap();
        recorder.push_video(0, true, vec![1]).unwrap();
        recorder.set_paused(true);
        assert_eq!(recorder.status(), Status::Paused);
        assert_eq!(
            recorder.push_video(33, false, vec![2]),
            Err(RecordError::Paused)
        );
        recorder.set_paused(false);
        recorder.set_audio_enabled(false);
        assert_eq!(
            recorder.push_audio(960, vec![3]),
            Ok(PushOutcome::DroppedMutedAudio)
        );
        let segment = recorder.drain_segment();
        assert_eq!(segment.len(), 1);
        recorder.cancel();
        assert_eq!(recorder.status(), Status::Cancelled);
        assert_eq!(recorder.queued_bytes(), 0);
    }

    #[test]
    fn bounds_config_packets_queue_and_duration() {
        let mut recorder = Recorder::new(Format::MatroskaH264Opus);
        assert_eq!(
            recorder.set_config(StreamConfig {
                width: 15,
                ..config(1)
            }),
            Err(RecordError::InvalidConfig)
        );
        recorder.set_config(config(1)).unwrap();
        assert_eq!(
            recorder.push_video(0, true, vec![0; MAX_PACKET_BYTES + 1]),
            Err(RecordError::PacketTooLarge)
        );
        recorder
            .push_video(0, true, vec![0; MAX_PACKET_BYTES])
            .unwrap();
        recorder
            .push_video(1, false, vec![0; MAX_PACKET_BYTES])
            .unwrap();
        recorder
            .push_video(2, false, vec![0; MAX_PACKET_BYTES])
            .unwrap();
        recorder
            .push_video(3, false, vec![0; MAX_PACKET_BYTES])
            .unwrap();
        assert_eq!(
            recorder.push_video(4, false, vec![0; MAX_PACKET_BYTES]),
            Err(RecordError::QueueFull)
        );
        recorder.drain_segment();
        recorder.set_config(config(1)).unwrap();
        recorder.push_video(0, true, vec![1]).unwrap();
        assert_eq!(
            recorder.push_video(MAX_DURATION_MS + 1, true, vec![1]),
            Err(RecordError::DurationLimit)
        );
    }

    fn test_directory() -> PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!("tabdisplay-recording-test-{nonce}"));
        fs::create_dir(&path).unwrap();
        path
    }

    #[test]
    fn destination_does_not_overwrite_and_commits_only_its_temp_file() {
        let dir = test_directory();
        let existing = dir.join("existing.mkv");
        fs::write(&existing, b"keep").unwrap();
        assert_eq!(
            Destination::prepare(&existing),
            Err(DestinationError::FinalExists)
        );

        let final_path = dir.join("session.mkv");
        let destination = Destination::prepare(&final_path).unwrap();
        let mut temp = destination.create_temp().unwrap();
        let temp_path = temp.temp_path().to_path_buf();
        assert!(temp_path.exists());
        assert!(temp_path
            .file_name()
            .unwrap()
            .to_string_lossy()
            .starts_with(".tabdisplay-recording-"));
        temp.write_bytes(b"container bytes from a future muxer")
            .unwrap();
        assert_eq!(temp.commit().unwrap(), final_path);
        assert!(!temp_path.exists());
        assert_eq!(
            fs::read(&final_path).unwrap(),
            b"container bytes from a future muxer"
        );
        assert_eq!(fs::read(&existing).unwrap(), b"keep");

        fs::remove_file(existing).unwrap();
        fs::remove_file(final_path).unwrap();
        fs::remove_dir(dir).unwrap();
    }

    #[test]
    fn cancel_and_drop_clean_only_owned_temp_files() {
        let dir = test_directory();
        let destination = Destination::prepare(dir.join("session.mkv")).unwrap();
        let temp_path = {
            let temp = destination.create_temp().unwrap();
            let path = temp.temp_path().to_path_buf();
            assert!(path.exists());
            temp.cancel().unwrap();
            path
        };
        assert!(!temp_path.exists());

        let dropped_path = {
            let temp = destination.create_temp().unwrap();
            temp.temp_path().to_path_buf()
        };
        assert!(!dropped_path.exists());
        fs::remove_dir(dir).unwrap();
    }

    #[test]
    fn writes_a_muxed_segment_only_into_its_owned_temp_file() {
        let dir = test_directory();
        let destination = Destination::prepare(dir.join("session.mkv")).unwrap();
        let mut temp = destination.create_temp().unwrap();
        let keyframe = vec![
            0, 0, 0, 1, 0x67, 0x42, 0, 0x1f, 0, 0, 1, 0x68, 0xce, 6, 0xe2, 0, 0, 1, 0x65, 0x88,
        ];
        temp.write_matroska_segment(
            &config(1),
            &[Packet::Video {
                pts_ms: 0,
                keyframe: true,
                data: keyframe,
            }],
        )
        .unwrap();
        let final_path = temp.commit().unwrap();
        let bytes = fs::read(&final_path).unwrap();
        assert_eq!(&bytes[..4], &[0x1a, 0x45, 0xdf, 0xa3]);
        fs::remove_file(final_path).unwrap();
        fs::remove_dir(dir).unwrap();
    }

    #[test]
    fn async_writer_commits_a_segment_without_blocking_the_submitter() {
        let dir = test_directory();
        let destination = Destination::prepare(dir.join("async-session.mkv")).unwrap();
        let keyframe = vec![
            0, 0, 0, 1, 0x67, 0x42, 0, 0x1f, 0, 0, 1, 0x68, 0xce, 6, 0xe2, 0, 0, 1, 0x65, 0x88,
        ];
        let writer = SegmentWriter::new();
        writer
            .submit(
                destination,
                config(1),
                vec![Packet::Video {
                    pts_ms: 0,
                    keyframe: true,
                    data: keyframe,
                }],
            )
            .unwrap();
        let paths = writer.finish().unwrap();
        assert_eq!(paths.len(), 1);
        let bytes = fs::read(&paths[0]).unwrap();
        assert_eq!(&bytes[..4], &[0x1a, 0x45, 0xdf, 0xa3]);
        assert!(dir.join("async-session.mkv").exists());
        fs::remove_file(&paths[0]).unwrap();
        fs::remove_dir(dir).unwrap();
    }

    #[test]
    fn destination_requires_a_real_parent_and_file_path() {
        let dir = test_directory();
        assert_eq!(
            Destination::prepare(dir.join("missing").join("session.mkv")),
            Err(DestinationError::ParentMissing)
        );
        assert_eq!(
            Destination::prepare(&dir),
            Err(DestinationError::FinalIsDirectory)
        );
        fs::remove_dir(dir).unwrap();
    }
}
