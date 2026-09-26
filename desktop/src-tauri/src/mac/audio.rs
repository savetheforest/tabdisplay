//! What the Mac plays, through a ScreenCaptureKit audio stream: 48 kHz stereo f32.
//! (The video side of the stream is a 2x2 placeholder: SCK needs a display to attach audio capture to.)
use screencapturekit::prelude::*;
use std::sync::{Arc, Mutex};
use std::time::Duration;

type Pcm = Arc<Mutex<Vec<f32>>>;

struct Handler(Pcm);

impl SCStreamOutputTrait for Handler {
    fn did_output_sample_buffer(&self, sample: CMSampleBuffer, kind: SCStreamOutputType) {
        if !matches!(kind, SCStreamOutputType::Audio) {
            return;
        }
        let Ok(list) = sample.audio_buffer_list() else { return };
        let mut pcm = self.0.lock().unwrap();
        let floats = |b: &screencapturekit::cm::AudioBuffer| -> Vec<f32> {
            b.data().chunks_exact(4).map(|c| f32::from_ne_bytes([c[0], c[1], c[2], c[3]])).collect()
        };
        if list.num_buffers() >= 2 {
            // Planar: one buffer per channel.
            let (l, r) = (floats(list.get(0).unwrap()), floats(list.get(1).unwrap()));
            pcm.extend(l.iter().zip(&r).flat_map(|(l, r)| [*l, *r]));
        } else if let Some(b) = list.get(0) {
            let samples = floats(b);
            if b.number_channels >= 2 {
                pcm.extend(samples); // already interleaved
            } else {
                pcm.extend(samples.iter().flat_map(|s| [*s, *s])); // mono
            }
        }
    }
}

pub struct Loopback {
    stream: SCStream,
    pcm: Pcm,
}

impl Drop for Loopback {
    fn drop(&mut self) {
        let _ = self.stream.stop_capture();
    }
}

impl Loopback {
    pub fn open() -> Result<Self, String> {
        let content = SCShareableContent::get().map_err(|e| format!("{e:?}"))?;
        let display = content.displays().into_iter().next().ok_or("sem monitor")?;
        let filter = SCContentFilter::create().with_display(&display).with_excluding_windows(&[]).build().map_err(|e| format!("{e:?}"))?;
        let config = SCStreamConfiguration::new()
            .with_width(2)
            .with_height(2)
            .with_minimum_frame_interval(&CMTime::new(1, 1))
            .with_captures_audio(true)
            .with_sample_rate(48000)
            .with_channel_count(2)
            .with_excludes_current_process_audio(true);
        let pcm = Pcm::default();
        let mut stream = SCStream::new(&filter, &config).map_err(|e| format!("{e:?}"))?;
        stream.add_output_handler(Handler(pcm.clone()), SCStreamOutputType::Audio).map_err(|e| format!("{e:?}"))?;
        stream.start_capture().map_err(|e| format!("{e:?}"))?;
        Ok(Self { stream, pcm })
    }

    /// Appends the interleaved samples captured since the last call (waits ~10 ms if there are none).
    pub fn read(&mut self, out: &mut Vec<f32>) -> Result<(), String> {
        let mut pcm = self.pcm.lock().unwrap();
        if pcm.is_empty() {
            drop(pcm);
            std::thread::sleep(Duration::from_millis(10));
            return Ok(());
        }
        out.append(&mut pcm);
        Ok(())
    }
}
