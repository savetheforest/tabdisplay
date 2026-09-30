use openh264::encoder::{
    BitRate, Complexity, Encoder, EncoderConfig, FrameRate, RateControlMode, UsageType,
};
use openh264::formats::{BgraSliceU8, YUVBuffer};
use openh264::OpenH264API;

// ponytail: software H.264 (openh264), measured ~23 ms/frame at 1080p, ~44 ms at 2560x1600 (12 cores,
// single-threaded — see `new()`: `.num_threads()` alone is a no-op unless `.max_slice_len()` is also
// set, since without it the encoder forces SM_SINGLE_SLICE and collapses back to 1 thread internally).
// Fine for the 1080p mirror; the tablet's native res needs Media Foundation / VideoToolbox hardware
// encoders, or as a fallback the multi-threaded slice encoding enabled below.
// CameraVideoRealTime is ~2x faster than ScreenContentRealTime here.
pub struct H264 {
    enc: Encoder,
    yuv: YUVBuffer,
    w: usize,
    h: usize,
}

impl H264 {
    pub fn new(w: usize, h: usize, fps: u32, bitrate: u32) -> Result<Self, openh264::Error> {
        // Slice the frame across threads (capped at 4: the vendored encoder's hard limit). Splitting by
        // a byte budget per slice, rather than by row count, is what actually makes it multi-threaded:
        // leaving `max_slice_len` unset forces a single slice, and `.num_threads()` alone does nothing.
        let threads = std::thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(1)
            .min(4) as u16;
        let slice_bytes = ((bitrate as u64 / 8 / fps.max(1) as u64) / threads.max(1) as u64)
            .clamp(4096, 262_144) as u32;
        let config = EncoderConfig::new()
            .usage_type(UsageType::CameraVideoRealTime)
            .rate_control_mode(RateControlMode::Bitrate)
            .bitrate(BitRate::from_bps(bitrate))
            .max_frame_rate(FrameRate::from_hz(fps as f32))
            .skip_frames(false)
            .complexity(Complexity::Low)
            .num_threads(threads)
            .max_slice_len(slice_bytes);
        let enc = Encoder::with_api_config(OpenH264API::from_source(), config)?;
        Ok(Self {
            enc,
            yuv: YUVBuffer::new(w, h),
            w,
            h,
        })
    }

    /// Encodes one BGRA frame into an Annex-B access unit (SPS/PPS included on keyframes).
    pub fn encode(&mut self, bgra: &[u8], out: &mut Vec<u8>) -> Result<(), openh264::Error> {
        self.yuv
            .read_bgra8(BgraSliceU8::new(bgra, (self.w, self.h)));
        out.clear();
        self.enc.encode(&self.yuv)?.write_vec(out);
        Ok(())
    }
}
