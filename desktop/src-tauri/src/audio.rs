//! System audio -> Opus. One capture thread per session encodes what the PC plays (48 kHz stereo, 20 ms frames)
//! and queues the packets; the streaming loop in `server.rs` is the only writer on the socket, so it sends them.
use crate::sys::audio::Loopback;
use opus_rs::{Application, OpusEncoder};
use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

pub const RATE: usize = 48_000;
pub const CHANNELS: usize = 2;
/// 20 ms of audio, per channel.
const FRAME: usize = RATE / 50;
const BITRATE: i32 = 128_000;
pub const FRAME_MS: u64 = 20;
pub const QUEUE_MS: u64 = 160;
const QUEUE_FRAMES: usize = (QUEUE_MS / FRAME_MS) as usize;

/// A packet stays self-contained: sequence is for loss detection and PTS is in 48 kHz samples.
#[derive(Clone, Debug)]
pub struct Packet {
    pub sequence: u64,
    pub pts_samples: u64,
    pub opus: Vec<u8>,
}

/// Bounded latest-audio queue. Oldest packets are removed when the writer falls behind, so delay
/// is bounded by time rather than by an incidental channel capacity.
#[derive(Clone, Default)]
pub struct Queue(Arc<Mutex<VecDeque<Packet>>>);

impl Queue {
    /// Removes the oldest packet only if it fits in the caller's remaining byte budget.
    /// Keeping the packet in the queue avoids silently losing it when a media turn is full.
    pub fn try_pop_with_budget(&self, used: usize, budget: usize) -> Option<Packet> {
        let mut queue = self.0.lock().ok()?;
        let packet = queue.front()?;
        if packet.opus.len() > budget.saturating_sub(used) {
            return None;
        }
        queue.pop_front()
    }

    pub fn push(&self, packet: Packet) {
        if let Ok(mut queue) = self.0.lock() {
            while queue.len() >= QUEUE_FRAMES {
                queue.pop_front();
            }
            queue.push_back(packet);
        }
    }

    pub fn clear(&self) {
        if let Ok(mut queue) = self.0.lock() {
            queue.clear();
        }
    }

    #[cfg(test)]
    fn len(&self) -> usize {
        self.0.lock().unwrap().len()
    }
}

/// Starts capturing while `alive()` and `enabled()` hold; the receiver yields one Opus packet per 20 ms.
pub fn spawn(
    alive: impl Fn() -> bool + Send + 'static,
    enabled: impl Fn() -> bool + Send + 'static,
) -> Queue {
    let queue = Queue::default();
    let output = queue.clone();
    thread::spawn(move || {
        let Ok(mut encoder) = OpusEncoder::new(RATE as i32, CHANNELS, Application::Audio) else {
            return;
        };
        encoder.bitrate_bps = BITRATE;
        let (mut loopback, mut pcm, mut packet, mut sequence, mut pts_samples) =
            (None, Vec::<f32>::new(), vec![0u8; 1500], 0u64, 0u64);
        let mut quiet = 0u32; // consecutive silent frames
        while alive() {
            if !enabled() {
                loopback = None; // release the device while audio is off
                output.clear();
                thread::sleep(Duration::from_millis(200));
                continue;
            }
            if loopback.is_none() {
                loopback = Loopback::open().ok();
                if loopback.is_none() {
                    thread::sleep(Duration::from_secs(1)); // no output device / no permission yet: retry
                    continue;
                }
            }
            if loopback.as_mut().unwrap().read(&mut pcm).is_err() {
                loopback = None;
                continue;
            }
            while pcm.len() >= FRAME * CHANNELS {
                // WASAPI hands out zeros while nothing plays: send a second of them (so tails ring out), then nothing.
                quiet = if pcm[..FRAME * CHANNELS].iter().all(|&v| v == 0.0) {
                    quiet + 1
                } else {
                    0
                };
                if quiet <= 50 {
                    if let Ok(n) = encoder.encode(&pcm[..FRAME * CHANNELS], FRAME, &mut packet) {
                        output.push(Packet {
                            sequence,
                            pts_samples,
                            opus: packet[..n].to_vec(),
                        });
                    }
                }
                sequence = sequence.wrapping_add(1);
                pts_samples = pts_samples.saturating_add(FRAME as u64);
                pcm.drain(..FRAME * CHANNELS);
            }
        }
    });
    queue
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn opus_roundtrip_keeps_the_tone() {
        let mut enc = OpusEncoder::new(RATE as i32, CHANNELS, Application::Audio).unwrap();
        enc.bitrate_bps = BITRATE;
        let mut dec = opus_rs::OpusDecoder::new(RATE as i32, CHANNELS).unwrap();
        let (mut packet, mut out) = (vec![0u8; 1500], vec![0f32; FRAME * CHANNELS]);
        let mut energy = 0.0;
        for n in 0..10 {
            let pcm: Vec<f32> = (0..FRAME)
                .flat_map(|i| {
                    let s = (2.0 * std::f32::consts::PI * 440.0 * (n * FRAME + i) as f32
                        / RATE as f32)
                        .sin()
                        * 0.5;
                    [s, s]
                })
                .collect();
            let len = enc.encode(&pcm, FRAME, &mut packet).unwrap();
            assert!(len > 0 && len < 1500);
            dec.decode(&packet[..len], FRAME, &mut out).unwrap();
            energy = out.iter().map(|v| v * v).sum::<f32>();
        }
        assert!(
            energy > 10.0,
            "decoded tone should not be silence, energy {energy}"
        );
    }

    #[test]
    fn queue_discards_oldest_with_a_time_budget() {
        let queue = Queue::default();
        for sequence in 0..(QUEUE_FRAMES as u64 + 2) {
            queue.push(Packet {
                sequence,
                pts_samples: sequence * FRAME as u64,
                opus: vec![sequence as u8],
            });
        }
        assert_eq!(queue.len(), QUEUE_FRAMES);
        assert_eq!(
            queue.try_pop_with_budget(0, usize::MAX).unwrap().sequence,
            2
        );
    }
}
