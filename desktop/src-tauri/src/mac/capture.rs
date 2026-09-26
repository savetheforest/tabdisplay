//! Screen capture through ScreenCaptureKit, copied to CPU as tightly packed BGRA.
//! Unlike Windows' Desktop Duplication, ScreenCaptureKit draws the mouse cursor into the frames.
use super::display::{CGDisplayBounds, CGMainDisplayID};
use screencapturekit::prelude::*;
use screencapturekit::SCFrameStatus;
use std::ffi::c_void;
use std::sync::{Arc, Condvar, Mutex};
use std::time::Duration;

#[link(name = "CoreGraphics", kind = "framework")]
unsafe extern "C" {
    fn CGDisplayCopyDisplayMode(display: u32) -> *mut c_void;
    fn CGDisplayModeGetPixelWidth(mode: *mut c_void) -> usize;
    fn CGDisplayModeGetPixelHeight(mode: *mut c_void) -> usize;
    fn CGDisplayModeRelease(mode: *mut c_void);
}

#[derive(Default)]
struct Latest {
    bgra: Vec<u8>,
    fresh: bool,
}

type Shared = Arc<(Mutex<Latest>, Condvar)>;

struct Handler {
    shared: Shared,
    width: usize,
    height: usize,
}

impl SCStreamOutputTrait for Handler {
    fn did_output_sample_buffer(&self, sample: CMSampleBuffer, _: SCStreamOutputType) {
        // Idle frames repeat the previous image: nothing new to send.
        if !matches!(sample.frame_status(), Some(SCFrameStatus::Complete)) {
            return;
        }
        let Some(pixels) = sample.pixel_buffer() else { return };
        let Ok(guard) = pixels.lock_read_only() else { return };
        let (w, h, stride) = (self.width.min(guard.width()), self.height.min(guard.height()), guard.bytes_per_row());
        let base = guard.base_address();
        if base.is_null() {
            return;
        }
        let (lock, ready) = &*self.shared;
        let mut latest = lock.lock().unwrap();
        latest.bgra.resize(self.width * self.height * 4, 0);
        let row = w * 4;
        for y in 0..h {
            let src = unsafe { std::slice::from_raw_parts(base.add(y * stride), row) };
            latest.bgra[y * self.width * 4..][..row].copy_from_slice(src);
        }
        latest.fresh = true;
        ready.notify_one();
    }
}

pub struct Capture {
    stream: SCStream,
    shared: Shared,
    pub width: usize,
    pub height: usize,
    /// Display rect in global points (what CGEvent input uses): left, top, right, bottom.
    pub rect: (i32, i32, i32, i32),
}

impl Drop for Capture {
    fn drop(&mut self) {
        let _ = self.stream.stop_capture();
    }
}

pub fn pixel_size(id: u32) -> (usize, usize) {
    unsafe {
        let mode = CGDisplayCopyDisplayMode(id);
        if mode.is_null() {
            let b = CGDisplayBounds(id);
            return (b.size.width as usize, b.size.height as usize);
        }
        let size = (CGDisplayModeGetPixelWidth(mode), CGDisplayModeGetPixelHeight(mode));
        CGDisplayModeRelease(mode);
        size
    }
}

impl Capture {
    /// Captures the display whose CGDirectDisplayID is `device`, or the main display if None.
    pub fn open(device: Option<&str>) -> Result<Self, String> {
        let content = SCShareableContent::get().map_err(|e| format!("sem permissão de Gravação de Tela? ({e:?})"))?;
        let want = device.and_then(|d| d.parse::<u32>().ok()).unwrap_or_else(|| unsafe { CGMainDisplayID() });
        let display = content.displays().into_iter().find(|d| d.display_id() == want).ok_or("monitor não encontrado")?;
        let id = display.display_id();
        let (width, height) = pixel_size(id);
        let b = unsafe { CGDisplayBounds(id) };
        let rect = (b.origin.x as i32, b.origin.y as i32, (b.origin.x + b.size.width) as i32, (b.origin.y + b.size.height) as i32);

        let filter = SCContentFilter::create().with_display(&display).with_excluding_windows(&[]).build().map_err(|e| format!("{e:?}"))?;
        let config = SCStreamConfiguration::new()
            .with_width(width as u32)
            .with_height(height as u32)
            .with_pixel_format(PixelFormat::BGRA)
            .with_shows_cursor(true)
            .with_minimum_frame_interval(&CMTime::new(1, 120))
            .with_queue_depth(3);
        let shared: Shared = Arc::default();
        let mut stream = SCStream::new(&filter, &config).map_err(|e| format!("{e:?}"))?;
        stream
            .add_output_handler(Handler { shared: shared.clone(), width, height }, SCStreamOutputType::Screen)
            .map_err(|e| format!("{e:?}"))?;
        stream.start_capture().map_err(|e| format!("{e:?}"))?;
        Ok(Self { stream, shared, width, height, rect })
    }

    /// Waits up to `timeout_ms` for a new frame and copies it into `buf`. Returns false on timeout.
    pub fn next(&mut self, buf: &mut Vec<u8>, timeout_ms: u32) -> Result<bool, String> {
        let (lock, ready) = &*self.shared;
        let latest = lock.lock().unwrap();
        let (mut latest, _) = ready.wait_timeout_while(latest, Duration::from_millis(timeout_ms as u64), |l| !l.fresh).unwrap();
        if !latest.fresh {
            return Ok(false);
        }
        latest.fresh = false;
        buf.clone_from(&latest.bgra);
        Ok(true)
    }
}

/// Displays that can be mirrored: (CGDirectDisplayID as text, width, height).
pub fn monitors() -> Vec<(String, u32, u32)> {
    let Ok(content) = SCShareableContent::get() else { return Vec::new() };
    content
        .displays()
        .into_iter()
        .map(|d| {
            let (w, h) = pixel_size(d.display_id());
            (d.display_id().to_string(), w as u32, h as u32)
        })
        .collect()
}
