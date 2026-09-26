//! System audio -> Opus. One capture thread per session encodes what the PC plays (48 kHz stereo, 20 ms frames)
//! and queues the packets; the streaming loop in `server.rs` is the only writer on the socket, so it sends them.
use crate::sys::audio::Loopback;
use opus_rs::{Application, OpusEncoder};
use std::sync::mpsc::{sync_channel, Receiver};
use std::thread;
use std::time::Duration;

pub const RATE: usize = 48_000;
pub const CHANNELS: usize = 2;
/// 20 ms of audio, per channel.
const FRAME: usize = RATE / 50;
const BITRATE: i32 = 128_000;

/// Starts capturing while `alive()` and `enabled()` hold; the receiver yields one Opus packet per 20 ms.
pub fn spawn(alive: impl Fn() -> bool + Send + 'static, enabled: impl Fn() -> bool + Send + 'static) -> Receiver<Vec<u8>> {
    // Bounded: if the streaming loop falls behind, stale audio is dropped instead of piling up as latency.
    let (tx, rx) = sync_channel(25);
    thread::spawn(move || {
        let Ok(mut encoder) = OpusEncoder::new(RATE as i32, CHANNELS, Application::Audio) else { return };
        encoder.bitrate_bps = BITRATE;
        let (mut loopback, mut pcm, mut packet) = (None, Vec::<f32>::new(), vec![0u8; 1500]);
        let mut quiet = 0u32; // consecutive silent frames
        while alive() {
            if !enabled() {
                loopback = None; // release the device while audio is off
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
                quiet = if pcm[..FRAME * CHANNELS].iter().all(|&v| v == 0.0) { quiet + 1 } else { 0 };
                if quiet <= 50 {
                    if let Ok(n) = encoder.encode(&pcm[..FRAME * CHANNELS], FRAME, &mut packet) {
                        let _ = tx.try_send(packet[..n].to_vec());
                    }
                }
                pcm.drain(..FRAME * CHANNELS);
            }
        }
    });
    rx
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
                    let s = (2.0 * std::f32::consts::PI * 440.0 * (n * FRAME + i) as f32 / RATE as f32).sin() * 0.5;
                    [s, s]
                })
                .collect();
            let len = enc.encode(&pcm, FRAME, &mut packet).unwrap();
            assert!(len > 0 && len < 1500);
            dec.decode(&packet[..len], FRAME, &mut out).unwrap();
            energy = out.iter().map(|v| v * v).sum::<f32>();
        }
        assert!(energy > 10.0, "decoded tone should not be silence, energy {energy}");
    }
}
