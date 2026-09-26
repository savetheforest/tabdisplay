//! Hardware H.264 through VideoToolbox (Apple silicon's media engine), in low-latency real-time mode.
//! VideoToolbox emits AVCC (length-prefixed NAL units, parameter sets on the side); the tablet wants
//! Annex-B with SPS/PPS before each keyframe, so each frame is rewritten on the way out.
use std::ffi::c_void;
use std::sync::Mutex;

type CFTypeRef = *const c_void;

#[repr(C)]
#[derive(Clone, Copy)]
struct CMTime {
    value: i64,
    timescale: i32,
    flags: u32,
    epoch: i64,
}

impl CMTime {
    const INVALID: Self = Self { value: 0, timescale: 0, flags: 0, epoch: 0 };
    fn new(value: i64, timescale: i32) -> Self {
        Self { value, timescale, flags: 1, epoch: 0 } // kCMTimeFlags_Valid
    }
}

type OutputCallback = extern "C" fn(*mut c_void, *mut c_void, i32, u32, CFTypeRef);

#[link(name = "CoreFoundation", kind = "framework")]
unsafe extern "C" {
    static kCFBooleanTrue: CFTypeRef;
    static kCFBooleanFalse: CFTypeRef;
    static kCFTypeDictionaryKeyCallBacks: c_void;
    static kCFTypeDictionaryValueCallBacks: c_void;
    fn CFNumberCreate(allocator: CFTypeRef, the_type: isize, value: *const c_void) -> CFTypeRef;
    fn CFDictionaryCreate(allocator: CFTypeRef, keys: *const CFTypeRef, values: *const CFTypeRef, count: isize, key_cb: *const c_void, value_cb: *const c_void) -> CFTypeRef;
    fn CFRelease(cf: CFTypeRef);
}

#[link(name = "CoreVideo", kind = "framework")]
unsafe extern "C" {
    static kCVPixelBufferPixelFormatTypeKey: CFTypeRef;
    static kCVPixelBufferWidthKey: CFTypeRef;
    static kCVPixelBufferHeightKey: CFTypeRef;
    fn CVPixelBufferPoolCreatePixelBuffer(allocator: CFTypeRef, pool: CFTypeRef, out: *mut CFTypeRef) -> i32;
    fn CVPixelBufferLockBaseAddress(buffer: CFTypeRef, flags: u64) -> i32;
    fn CVPixelBufferUnlockBaseAddress(buffer: CFTypeRef, flags: u64) -> i32;
    fn CVPixelBufferGetBaseAddress(buffer: CFTypeRef) -> *mut u8;
    fn CVPixelBufferGetBytesPerRow(buffer: CFTypeRef) -> usize;
}

#[link(name = "VideoToolbox", kind = "framework")]
unsafe extern "C" {
    static kVTCompressionPropertyKey_RealTime: CFTypeRef;
    static kVTCompressionPropertyKey_AllowFrameReordering: CFTypeRef;
    static kVTCompressionPropertyKey_AverageBitRate: CFTypeRef;
    static kVTCompressionPropertyKey_ExpectedFrameRate: CFTypeRef;
    static kVTCompressionPropertyKey_MaxKeyFrameInterval: CFTypeRef;
    static kVTCompressionPropertyKey_ProfileLevel: CFTypeRef;
    static kVTProfileLevel_H264_Main_AutoLevel: CFTypeRef;
    static kVTVideoEncoderSpecification_EnableLowLatencyRateControl: CFTypeRef;
    fn VTCompressionSessionCreate(
        allocator: CFTypeRef,
        width: i32,
        height: i32,
        codec: u32,
        encoder_spec: CFTypeRef,
        source_attrs: CFTypeRef,
        compressed_allocator: CFTypeRef,
        callback: OutputCallback,
        refcon: *mut c_void,
        session: *mut CFTypeRef,
    ) -> i32;
    fn VTSessionSetProperty(session: CFTypeRef, key: CFTypeRef, value: CFTypeRef) -> i32;
    fn VTCompressionSessionPrepareToEncodeFrames(session: CFTypeRef) -> i32;
    fn VTCompressionSessionGetPixelBufferPool(session: CFTypeRef) -> CFTypeRef;
    fn VTCompressionSessionEncodeFrame(session: CFTypeRef, image: CFTypeRef, pts: CMTime, duration: CMTime, props: CFTypeRef, frame_refcon: *mut c_void, info: *mut u32) -> i32;
    fn VTCompressionSessionCompleteFrames(session: CFTypeRef, until: CMTime) -> i32;
    fn VTCompressionSessionInvalidate(session: CFTypeRef);
}

#[link(name = "CoreMedia", kind = "framework")]
unsafe extern "C" {
    fn CMSampleBufferGetDataBuffer(sample: CFTypeRef) -> CFTypeRef;
    fn CMSampleBufferGetFormatDescription(sample: CFTypeRef) -> CFTypeRef;
    fn CMBlockBufferGetDataLength(block: CFTypeRef) -> usize;
    fn CMBlockBufferCopyDataBytes(block: CFTypeRef, offset: usize, length: usize, dest: *mut c_void) -> i32;
    fn CMVideoFormatDescriptionGetH264ParameterSetAtIndex(desc: CFTypeRef, index: usize, set: *mut *const u8, size: *mut usize, count: *mut usize, nal_header_len: *mut i32) -> i32;
}

const AVC1: u32 = u32::from_be_bytes(*b"avc1");
const BGRA: i32 = i32::from_be_bytes(*b"BGRA");
const START: [u8; 4] = [0, 0, 0, 1];

fn number(v: i32) -> CFTypeRef {
    unsafe { CFNumberCreate(std::ptr::null(), 3, (&v as *const i32).cast()) } // kCFNumberSInt32Type
}

fn dict(pairs: &[(CFTypeRef, CFTypeRef)]) -> CFTypeRef {
    let (keys, values): (Vec<_>, Vec<_>) = pairs.iter().copied().unzip();
    unsafe {
        CFDictionaryCreate(std::ptr::null(), keys.as_ptr(), values.as_ptr(), pairs.len() as isize, &kCFTypeDictionaryKeyCallBacks, &kCFTypeDictionaryValueCallBacks)
    }
}

pub struct HwEncoder {
    session: CFTypeRef,
    /// Filled by the output callback during `encode` (boxed: its address is the callback's refcon).
    out: Box<Mutex<Vec<u8>>>,
    w: usize,
    h: usize,
    fps: u32,
    frame: i64,
}

// The session is used from the one session thread; VideoToolbox's callback only touches `out`.
unsafe impl Send for HwEncoder {}

impl HwEncoder {
    pub fn new(w: usize, h: usize, fps: u32, bitrate: u32) -> Result<Self, String> {
        let out: Box<Mutex<Vec<u8>>> = Box::default();
        let mut session = std::ptr::null();
        unsafe {
            let spec = dict(&[(kVTVideoEncoderSpecification_EnableLowLatencyRateControl, kCFBooleanTrue)]);
            let (pf, wn, hn) = (number(BGRA), number(w as i32), number(h as i32));
            let attrs = dict(&[(kCVPixelBufferPixelFormatTypeKey, pf), (kCVPixelBufferWidthKey, wn), (kCVPixelBufferHeightKey, hn)]);
            let status = VTCompressionSessionCreate(
                std::ptr::null(),
                w as i32,
                h as i32,
                AVC1,
                spec,
                attrs,
                std::ptr::null(),
                on_output,
                (&*out as *const Mutex<Vec<u8>>) as *mut c_void,
                &mut session,
            );
            for cf in [spec, attrs, pf, wn, hn] {
                CFRelease(cf);
            }
            if status != 0 {
                return Err(format!("VideoToolbox recusou {w}x{h} (erro {status})"));
            }
            let set = |key: CFTypeRef, value: CFTypeRef| VTSessionSetProperty(session, key, value);
            set(kVTCompressionPropertyKey_RealTime, kCFBooleanTrue);
            set(kVTCompressionPropertyKey_AllowFrameReordering, kCFBooleanFalse); // no B-frames: lowest latency
            set(kVTCompressionPropertyKey_ProfileLevel, kVTProfileLevel_H264_Main_AutoLevel);
            for (key, v) in [
                (kVTCompressionPropertyKey_AverageBitRate, bitrate as i32),
                (kVTCompressionPropertyKey_ExpectedFrameRate, fps as i32),
                (kVTCompressionPropertyKey_MaxKeyFrameInterval, fps as i32 * 2), // a keyframe every 2 s heals lost frames
            ] {
                let n = number(v);
                set(key, n);
                CFRelease(n);
            }
            VTCompressionSessionPrepareToEncodeFrames(session);
        }
        Ok(Self { session, out, w, h, fps: fps.max(1), frame: 0 })
    }

    /// Encodes one BGRA frame into an Annex-B access unit.
    pub fn encode(&mut self, bgra: &[u8], out: &mut Vec<u8>) -> Result<(), String> {
        out.clear();
        unsafe {
            let pool = VTCompressionSessionGetPixelBufferPool(self.session);
            let mut pixels = std::ptr::null();
            if pool.is_null() || CVPixelBufferPoolCreatePixelBuffer(std::ptr::null(), pool, &mut pixels) != 0 {
                return Err("sem buffer de vídeo".into());
            }
            CVPixelBufferLockBaseAddress(pixels, 0);
            let (base, stride, row) = (CVPixelBufferGetBaseAddress(pixels), CVPixelBufferGetBytesPerRow(pixels), self.w * 4);
            for y in 0..self.h {
                std::ptr::copy_nonoverlapping(bgra.as_ptr().add(y * row), base.add(y * stride), row);
            }
            CVPixelBufferUnlockBaseAddress(pixels, 0);

            let pts = CMTime::new(self.frame, self.fps as i32);
            self.frame += 1;
            let status = VTCompressionSessionEncodeFrame(self.session, pixels, pts, CMTime::new(1, self.fps as i32), std::ptr::null(), std::ptr::null_mut(), std::ptr::null_mut());
            // Wait for this frame: VideoToolbox calls `on_output` before this returns.
            VTCompressionSessionCompleteFrames(self.session, CMTime::INVALID);
            CFRelease(pixels);
            if status != 0 {
                return Err(format!("VideoToolbox falhou (erro {status})"));
            }
        }
        std::mem::swap(out, &mut self.out.lock().unwrap());
        Ok(())
    }
}

impl Drop for HwEncoder {
    fn drop(&mut self) {
        unsafe {
            VTCompressionSessionInvalidate(self.session);
            CFRelease(self.session);
        }
    }
}

extern "C" fn on_output(refcon: *mut c_void, _frame: *mut c_void, status: i32, _flags: u32, sample: CFTypeRef) {
    if status != 0 || sample.is_null() {
        return;
    }
    let out = unsafe { &*(refcon as *const Mutex<Vec<u8>>) };
    let mut out = out.lock().unwrap();
    unsafe {
        let block = CMSampleBufferGetDataBuffer(sample);
        let len = CMBlockBufferGetDataLength(block);
        let mut avcc = vec![0u8; len];
        if CMBlockBufferCopyDataBytes(block, 0, len, avcc.as_mut_ptr().cast()) != 0 {
            return;
        }
        let nals = split_avcc(&avcc);
        if nals.iter().any(|n| n.first().is_some_and(|b| b & 0x1f == 5)) {
            // Keyframe: put SPS and PPS in front so a decoder can start right here.
            let desc = CMSampleBufferGetFormatDescription(sample);
            let mut count = 0;
            let (mut p, mut n) = (std::ptr::null(), 0);
            CMVideoFormatDescriptionGetH264ParameterSetAtIndex(desc, 0, &mut p, &mut n, &mut count, std::ptr::null_mut());
            for i in 0..count {
                if CMVideoFormatDescriptionGetH264ParameterSetAtIndex(desc, i, &mut p, &mut n, std::ptr::null_mut(), std::ptr::null_mut()) == 0 {
                    out.extend_from_slice(&START);
                    out.extend_from_slice(std::slice::from_raw_parts(p, n));
                }
            }
        }
        for nal in nals {
            out.extend_from_slice(&START);
            out.extend_from_slice(nal);
        }
    }
}

/// AVCC: each NAL unit is prefixed by its 4-byte big-endian length.
fn split_avcc(data: &[u8]) -> Vec<&[u8]> {
    let mut nals = Vec::new();
    let mut i = 0;
    while i + 4 <= data.len() {
        let n = u32::from_be_bytes(data[i..i + 4].try_into().unwrap()) as usize;
        let Some(nal) = data.get(i + 4..i + 4 + n) else { break };
        nals.push(nal);
        i += 4 + n;
    }
    nals
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_avcc() {
        let data = [0, 0, 0, 2, 0x65, 0xAA, 0, 0, 0, 1, 0x41, 0, 0, 0, 9]; // last length overruns: ignored
        assert_eq!(split_avcc(&data), vec![&[0x65, 0xAA][..], &[0x41][..]]);
    }
}
