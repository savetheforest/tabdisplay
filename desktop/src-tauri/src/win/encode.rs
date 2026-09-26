//! Hardware H.264 through the GPU vendor's async Media Foundation encoder MFT (AMD/NVIDIA/Intel).
use openh264::formats::{BgraSliceU8, YUVBuffer, YUVSource};
use std::time::{Duration, Instant};
use windows::core::{Error, Interface, Result};
use windows::Win32::Media::MediaFoundation::*;
use windows::Win32::System::Com::{CoInitializeEx, CoTaskMemFree, COINIT_MULTITHREADED};
use windows::Win32::System::Variant::VARIANT;

pub type HwEncoder = MfEncoder;

pub struct MfEncoder {
    mft: IMFTransform,
    events: IMFMediaEventGenerator,
    yuv: YUVBuffer, // BGRA -> I420 via openh264's SIMD converter, then interleaved to NV12
    w: usize,
    h: usize,
    wanted_inputs: u32,
    frame: i64,
    fps: u32,
    provides_samples: bool,
}

impl MfEncoder {
    pub fn new(w: usize, h: usize, fps: u32, bitrate: u32) -> Result<Self> {
        unsafe {
            let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
            MFStartup(MF_VERSION, MFSTARTUP_LITE)?;

            let input = MFT_REGISTER_TYPE_INFO { guidMajorType: MFMediaType_Video, guidSubtype: MFVideoFormat_NV12 };
            let output = MFT_REGISTER_TYPE_INFO { guidMajorType: MFMediaType_Video, guidSubtype: MFVideoFormat_H264 };
            let (mut list, mut n) = (std::ptr::null_mut(), 0);
            MFTEnumEx(
                MFT_CATEGORY_VIDEO_ENCODER,
                MFT_ENUM_FLAG(MFT_ENUM_FLAG_HARDWARE.0 | MFT_ENUM_FLAG_SORTANDFILTER.0),
                Some(&input),
                Some(&output),
                &mut list,
                &mut n,
            )?;
            let activates = std::slice::from_raw_parts_mut(list, n as usize);
            let mft = match activates.first().and_then(|a| a.as_ref()) {
                Some(a) => a.ActivateObject::<IMFTransform>(),
                None => Err(Error::new(MF_E_NOT_FOUND, "no hardware H.264 encoder")),
            };
            activates.iter_mut().for_each(|a| drop(a.take()));
            CoTaskMemFree(Some(list as _));
            let mft = mft?;

            let attrs = mft.GetAttributes()?;
            attrs.SetUINT32(&MF_TRANSFORM_ASYNC_UNLOCK, 1)?;
            attrs.SetUINT32(&MF_LOW_LATENCY, 1)?;
            // Best effort: not every vendor supports every knob.
            if let Ok(codec) = mft.cast::<ICodecAPI>() {
                let _ = codec.SetValue(&CODECAPI_AVLowLatencyMode, &VARIANT::from(true));
                let _ = codec.SetValue(&CODECAPI_AVEncCommonRateControlMode, &VARIANT::from(eAVEncCommonRateControlMode_CBR.0 as u32));
                let _ = codec.SetValue(&CODECAPI_AVEncMPVGOPSize, &VARIANT::from(fps * 2)); // keyframe every 2s heals dropped frames
            }

            let size = ((w as u64) << 32) | h as u64;
            let rate = ((fps as u64) << 32) | 1;
            let out = MFCreateMediaType()?;
            out.SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Video)?;
            out.SetGUID(&MF_MT_SUBTYPE, &MFVideoFormat_H264)?;
            out.SetUINT32(&MF_MT_AVG_BITRATE, bitrate)?;
            out.SetUINT64(&MF_MT_FRAME_SIZE, size)?;
            out.SetUINT64(&MF_MT_FRAME_RATE, rate)?;
            out.SetUINT32(&MF_MT_INTERLACE_MODE, MFVideoInterlace_Progressive.0 as u32)?;
            out.SetUINT32(&MF_MT_MPEG2_PROFILE, eAVEncH264VProfile_Base.0 as u32)?; // baseline: no B-frames
            mft.SetOutputType(0, &out, 0)?;

            let inp = MFCreateMediaType()?;
            inp.SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Video)?;
            inp.SetGUID(&MF_MT_SUBTYPE, &MFVideoFormat_NV12)?;
            inp.SetUINT64(&MF_MT_FRAME_SIZE, size)?;
            inp.SetUINT64(&MF_MT_FRAME_RATE, rate)?;
            inp.SetUINT32(&MF_MT_INTERLACE_MODE, MFVideoInterlace_Progressive.0 as u32)?;
            mft.SetInputType(0, &inp, 0)?;

            let provides_samples = mft.GetOutputStreamInfo(0)?.dwFlags & MFT_OUTPUT_STREAM_PROVIDES_SAMPLES.0 as u32 != 0;
            mft.ProcessMessage(MFT_MESSAGE_NOTIFY_BEGIN_STREAMING, 0)?;
            mft.ProcessMessage(MFT_MESSAGE_NOTIFY_START_OF_STREAM, 0)?;

            Ok(Self {
                events: mft.cast()?,
                mft,
                yuv: YUVBuffer::new(w, h),
                w,
                h,
                wanted_inputs: 0,
                frame: 0,
                fps,
                provides_samples,
            })
        }
    }

    /// Encodes one BGRA frame into Annex-B H.264. `out` may stay empty if the encoder is still buffering.
    pub fn encode(&mut self, bgra: &[u8], out: &mut Vec<u8>) -> Result<()> {
        out.clear();
        let sample = self.nv12_sample(bgra)?;
        unsafe {
            while self.wanted_inputs == 0 {
                self.pump(true, out)?;
            }
            self.mft.ProcessInput(0, &sample, 0)?;
            self.wanted_inputs -= 1;
            // Low-latency encoders emit this frame right away; don't stall the capture loop if one doesn't.
            let deadline = Instant::now() + Duration::from_millis(50);
            while out.is_empty() && Instant::now() < deadline {
                if !self.pump(false, out)? {
                    std::thread::sleep(Duration::from_millis(1));
                }
            }
        }
        Ok(())
    }

    /// Handles one encoder event. Returns false if `wait` is false and nothing was pending.
    unsafe fn pump(&mut self, wait: bool, out: &mut Vec<u8>) -> Result<bool> {
        let flags = if wait { MEDIA_EVENT_GENERATOR_GET_EVENT_FLAGS(0) } else { MF_EVENT_FLAG_NO_WAIT };
        let event = match self.events.GetEvent(flags) {
            Err(e) if e.code() == MF_E_NO_EVENTS_AVAILABLE => return Ok(false),
            r => r?,
        };
        match MF_EVENT_TYPE(event.GetType()? as i32) {
            t if t == METransformNeedInput => self.wanted_inputs += 1,
            t if t == METransformHaveOutput => self.drain(out)?,
            _ => {}
        }
        Ok(true)
    }

    unsafe fn drain(&mut self, out: &mut Vec<u8>) -> Result<()> {
        let sample = if self.provides_samples {
            None
        } else {
            let s = MFCreateSample()?;
            s.AddBuffer(&MFCreateMemoryBuffer((self.w * self.h) as u32)?)?;
            Some(s)
        };
        let mut buf = [MFT_OUTPUT_DATA_BUFFER { dwStreamID: 0, pSample: std::mem::ManuallyDrop::new(sample), ..Default::default() }];
        let mut status = 0;
        let r = self.mft.ProcessOutput(0, &mut buf, &mut status);
        let [buf] = buf;
        let sample = std::mem::ManuallyDrop::into_inner(buf.pSample);
        drop(std::mem::ManuallyDrop::into_inner(buf.pEvents));
        if let Err(e) = r {
            if e.code() == MF_E_TRANSFORM_STREAM_CHANGE {
                // Encoder renegotiated its output type (some do on the first frame): accept it.
                return self.mft.SetOutputType(0, &self.mft.GetOutputAvailableType(0, 0)?, 0);
            }
            return Err(e);
        }
        let Some(sample) = sample else { return Ok(()) };
        let mb = sample.ConvertToContiguousBuffer()?;
        let (mut ptr, mut len) = (std::ptr::null_mut(), 0);
        mb.Lock(&mut ptr, None, Some(&mut len))?;
        out.extend_from_slice(std::slice::from_raw_parts(ptr, len as usize));
        mb.Unlock()?;
        Ok(())
    }

    fn nv12_sample(&mut self, bgra: &[u8]) -> Result<IMFSample> {
        self.yuv.read_bgra8(BgraSliceU8::new(bgra, (self.w, self.h)));
        let (w, h) = (self.w, self.h);
        let (ys, us, vs) = self.yuv.strides();
        let len = w * h * 3 / 2;
        unsafe {
            let mb = MFCreateMemoryBuffer(len as u32)?;
            let mut ptr = std::ptr::null_mut();
            mb.Lock(&mut ptr, None, None)?;
            let dst = std::slice::from_raw_parts_mut(ptr, len);
            let (y_dst, uv_dst) = dst.split_at_mut(w * h);
            for (row, d) in y_dst.chunks_exact_mut(w).enumerate() {
                d.copy_from_slice(&self.yuv.y()[row * ys..row * ys + w]);
            }
            for (row, d) in uv_dst.chunks_exact_mut(w).enumerate() {
                let (u, v) = (&self.yuv.u()[row * us..], &self.yuv.v()[row * vs..]);
                for (i, pair) in d.chunks_exact_mut(2).enumerate() {
                    pair[0] = u[i];
                    pair[1] = v[i];
                }
            }
            mb.Unlock()?;
            mb.SetCurrentLength(len as u32)?;

            let sample = MFCreateSample()?;
            sample.AddBuffer(&mb)?;
            sample.SetSampleTime(self.frame * 10_000_000 / self.fps as i64)?;
            sample.SetSampleDuration(10_000_000 / self.fps as i64)?;
            self.frame += 1;
            Ok(sample)
        }
    }
}

impl Drop for MfEncoder {
    fn drop(&mut self) {
        unsafe {
            let _ = self.mft.ProcessMessage(MFT_MESSAGE_NOTIFY_END_OF_STREAM, 0);
            let _ = MFShutdown();
        }
    }
}
