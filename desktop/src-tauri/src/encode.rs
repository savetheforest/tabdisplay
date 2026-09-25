use openh264::encoder::{BitRate, Complexity, Encoder, EncoderConfig, FrameRate, RateControlMode, UsageType};
use openh264::formats::{BgraSliceU8, YUVBuffer};
use openh264::OpenH264API;

// ponytail: software H.264 (openh264), measured ~23 ms/frame at 1080p, ~44 ms at 2560x1600 (12 cores).
// Fine for the 1080p mirror; the tablet's native res needs Media Foundation / VideoToolbox hardware encoders.
// CameraVideoRealTime is ~2x faster than ScreenContentRealTime here.
pub struct H264 {
    enc: Encoder,
    yuv: YUVBuffer,
    w: usize,
    h: usize,
}

impl H264 {
    pub fn new(w: usize, h: usize, fps: u32, bitrate: u32) -> Result<Self, openh264::Error> {
        let config = EncoderConfig::new()
            .usage_type(UsageType::CameraVideoRealTime)
            .rate_control_mode(RateControlMode::Bitrate)
            .bitrate(BitRate::from_bps(bitrate))
            .max_frame_rate(FrameRate::from_hz(fps as f32))
            .skip_frames(false)
            .complexity(Complexity::Low);
        let enc = Encoder::with_api_config(OpenH264API::from_source(), config)?;
        Ok(Self { enc, yuv: YUVBuffer::new(w, h), w, h })
    }

    /// Encodes one BGRA frame into an Annex-B access unit (SPS/PPS included on keyframes).
    pub fn encode(&mut self, bgra: &[u8], out: &mut Vec<u8>) -> Result<(), openh264::Error> {
        self.yuv.read_bgra8(BgraSliceU8::new(bgra, (self.w, self.h)));
        out.clear();
        self.enc.encode(&self.yuv)?.write_vec(out);
        Ok(())
    }
}

