//! What the PC plays, through WASAPI loopback on the default output device: 48 kHz stereo f32.
use windows::core::Result;
use windows::Win32::Media::Audio::*;
use windows::Win32::System::Com::{
    CoCreateInstance, CoInitializeEx, CLSCTX_ALL, COINIT_MULTITHREADED,
};

pub struct Loopback {
    client: IAudioClient,
    capture: IAudioCaptureClient,
}

impl Drop for Loopback {
    fn drop(&mut self) {
        let _ = unsafe { self.client.Stop() };
    }
}

impl Loopback {
    pub fn open() -> Result<Self> {
        unsafe {
            let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
            let enumerator: IMMDeviceEnumerator =
                CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)?;
            let device = enumerator.GetDefaultAudioEndpoint(eRender, eConsole)?;
            let client: IAudioClient = device.Activate(CLSCTX_ALL, None)?;
            // Ask for our format; Windows converts (rate, channels) from the device's mix format.
            let format = WAVEFORMATEX {
                wFormatTag: 3, // WAVE_FORMAT_IEEE_FLOAT
                nChannels: 2,
                nSamplesPerSec: 48_000,
                nAvgBytesPerSec: 48_000 * 8,
                nBlockAlign: 8,
                wBitsPerSample: 32,
                cbSize: 0,
            };
            client.Initialize(
                AUDCLNT_SHAREMODE_SHARED,
                AUDCLNT_STREAMFLAGS_LOOPBACK
                    | AUDCLNT_STREAMFLAGS_AUTOCONVERTPCM
                    | AUDCLNT_STREAMFLAGS_SRC_DEFAULT_QUALITY,
                1_000_000, // 100 ms buffer, in 100 ns units
                0,
                &format,
                None,
            )?;
            let capture: IAudioCaptureClient = client.GetService()?;
            client.Start()?;
            Ok(Self { client, capture })
        }
    }

    /// Appends the interleaved samples captured since the last call (waits ~10 ms if there are none:
    /// loopback delivers nothing while the PC is silent).
    pub fn read(&mut self, out: &mut Vec<f32>) -> Result<()> {
        unsafe {
            let mut got = false;
            while self.capture.GetNextPacketSize()? > 0 {
                let (mut data, mut frames, mut flags) = (std::ptr::null_mut(), 0, 0);
                self.capture
                    .GetBuffer(&mut data, &mut frames, &mut flags, None, None)?;
                let samples = frames as usize * 2;
                if flags & AUDCLNT_BUFFERFLAGS_SILENT.0 as u32 != 0 || data.is_null() {
                    out.resize(out.len() + samples, 0.0);
                } else {
                    out.extend_from_slice(std::slice::from_raw_parts(data as *const f32, samples));
                }
                self.capture.ReleaseBuffer(frames)?;
                got = true;
            }
            if !got {
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Needs an output device and something playing: `cargo test loopback -- --ignored` while music plays.
    #[test]
    #[ignore]
    fn loopback_captures_what_plays() {
        let mut lb = Loopback::open().expect("default output device");
        let mut pcm = Vec::new();
        for _ in 0..200 {
            lb.read(&mut pcm).unwrap();
        }
        let peak = pcm.iter().fold(0f32, |m, v| m.max(v.abs()));
        eprintln!("{} samples, peak {peak}", pcm.len());
        assert!(!pcm.is_empty());
    }
}
