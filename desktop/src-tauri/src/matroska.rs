//! Small, deterministic Matroska muxer for one bounded recording segment.
//!
//! The streaming path still owns no writer. This module only turns the
//! already bounded H.264 Annex-B/Opus packets into a valid Matroska byte
//! stream, so a future destination can write it incrementally without
//! concatenating unrelated codec bytes as if they were an MP4.

use crate::recording::{Packet, StreamConfig, MAX_DURATION_MS, MAX_PACKET_BYTES, MAX_QUEUE_BYTES};

const TIME_CODE_SCALE_NS: u64 = 1_000_000; // one Matroska tick is one millisecond
const CLUSTER_SPAN_MS: u64 = 30_000;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MuxError {
    InvalidConfig,
    EmptySegment,
    MissingKeyframe,
    MissingParameterSets,
    InvalidAnnexB,
    UnexpectedAudio,
    PacketTooLarge,
    QueueTooLarge,
    NonMonotonicTimestamp,
    DurationLimit,
    TimestampOutOfRange,
}

/// Encodes one complete segment. The first video packet must be an IDR that
/// carries SPS/PPS; callers should use `Recorder::drain_segment` as the
/// bounded handoff and start a new file when it reports a new configuration.
pub fn encode_segment(config: &StreamConfig, packets: &[Packet]) -> Result<Vec<u8>, MuxError> {
    if !config.valid() {
        return Err(MuxError::InvalidConfig);
    }
    if packets.is_empty() {
        return Err(MuxError::EmptySegment);
    }

    let mut ordered = Vec::with_capacity(packets.len());
    let mut first_video = true;
    let mut first_video_pts = None;
    let mut last_video_pts = None;
    let mut last_audio_pts = None;
    let mut last_media_pts_ms: Option<u64> = None;
    let mut parameter_sets = None;
    let mut packet_bytes = 0usize;

    for packet in packets {
        match packet {
            Packet::Video {
                pts_ms,
                keyframe,
                data,
            } => {
                if data.is_empty() || data.len() > MAX_PACKET_BYTES {
                    return Err(MuxError::PacketTooLarge);
                }
                packet_bytes = packet_bytes
                    .checked_add(data.len())
                    .filter(|bytes| *bytes <= MAX_QUEUE_BYTES)
                    .ok_or(MuxError::QueueTooLarge)?;
                if last_video_pts.is_some_and(|last| *pts_ms < last) {
                    return Err(MuxError::NonMonotonicTimestamp);
                }
                if first_video && !keyframe {
                    return Err(MuxError::MissingKeyframe);
                }
                if *keyframe && parameter_sets.is_none() {
                    parameter_sets = Some(extract_parameter_sets(data)?);
                }
                if first_video {
                    first_video_pts = Some(*pts_ms);
                    first_video = false;
                }
                last_video_pts = Some(*pts_ms);
                last_media_pts_ms =
                    Some(last_media_pts_ms.map_or(*pts_ms, |last| last.max(*pts_ms)));
                ordered.push((*pts_ms, 1u8, packet));
            }
            Packet::Audio { pts_samples, data } => {
                if !config.audio {
                    return Err(MuxError::UnexpectedAudio);
                }
                if first_video {
                    return Err(MuxError::MissingKeyframe);
                }
                if data.is_empty() || data.len() > MAX_PACKET_BYTES {
                    return Err(MuxError::PacketTooLarge);
                }
                packet_bytes = packet_bytes
                    .checked_add(data.len())
                    .filter(|bytes| *bytes <= MAX_QUEUE_BYTES)
                    .ok_or(MuxError::QueueTooLarge)?;
                if last_audio_pts.is_some_and(|last| *pts_samples < last) {
                    return Err(MuxError::NonMonotonicTimestamp);
                }
                last_audio_pts = Some(*pts_samples);
                let pts_ms = *pts_samples / 48;
                last_media_pts_ms = Some(last_media_pts_ms.map_or(pts_ms, |last| last.max(pts_ms)));
                ordered.push((pts_ms, 2u8, packet));
            }
        }
    }

    let parameter_sets = parameter_sets.ok_or(MuxError::MissingParameterSets)?;
    let first_video_pts = first_video_pts.ok_or(MuxError::MissingKeyframe)?;
    if last_video_pts.is_some_and(|last| last.saturating_sub(first_video_pts) > MAX_DURATION_MS)
        || last_media_pts_ms
            .is_some_and(|last| last.saturating_sub(first_video_pts) > MAX_DURATION_MS)
    {
        return Err(MuxError::DurationLimit);
    }
    ordered.sort_by_key(|(pts_ms, track, _)| (*pts_ms, *track));

    let mut body = info(config);
    body.extend(tracks(config, &parameter_sets));
    body.extend(clusters(&ordered)?);

    let mut output = ebml_header();
    output.extend(id_bytes(0x1853_8067));
    output.extend(unknown_size());
    output.extend(body);
    Ok(output)
}

fn extract_parameter_sets(data: &[u8]) -> Result<(Vec<u8>, Vec<u8>), MuxError> {
    let nals = annex_b_nals(data)?;
    let sps = nals
        .iter()
        .find(|nal| nal.first().is_some_and(|byte| byte & 0x1f == 7))
        .filter(|nal| nal.len() >= 4)
        .map(|nal| nal.to_vec());
    let pps = nals
        .iter()
        .find(|nal| nal.first().is_some_and(|byte| byte & 0x1f == 8))
        .filter(|nal| !nal.is_empty())
        .map(|nal| nal.to_vec());
    match (sps, pps) {
        (Some(sps), Some(pps)) => Ok((sps, pps)),
        _ => Err(MuxError::MissingParameterSets),
    }
}

fn annex_b_nals(data: &[u8]) -> Result<Vec<&[u8]>, MuxError> {
    let mut starts = Vec::new();
    let mut index = 0;
    while index + 3 <= data.len() {
        if index + 4 <= data.len() && data[index..index + 4] == [0, 0, 0, 1] {
            starts.push((index, 4));
            index += 4;
        } else if data[index..index + 3] == [0, 0, 1] {
            starts.push((index, 3));
            index += 3;
        } else {
            index += 1;
        }
    }
    if starts.is_empty() {
        return Err(MuxError::InvalidAnnexB);
    }
    let mut nals = Vec::with_capacity(starts.len());
    for (position, (start, prefix)) in starts.iter().enumerate() {
        let begin = start + prefix;
        let end = starts
            .get(position + 1)
            .map_or(data.len(), |(next, _)| *next);
        if begin >= end {
            return Err(MuxError::InvalidAnnexB);
        }
        nals.push(&data[begin..end]);
    }
    Ok(nals)
}

fn avc_access_unit(data: &[u8]) -> Result<Vec<u8>, MuxError> {
    let nals = annex_b_nals(data)?;
    let mut output = Vec::with_capacity(data.len());
    for nal in nals {
        let length = u32::try_from(nal.len()).map_err(|_| MuxError::PacketTooLarge)?;
        output.extend(length.to_be_bytes());
        output.extend(nal);
    }
    Ok(output)
}

fn avc_codec_private((sps, pps): &(Vec<u8>, Vec<u8>)) -> Vec<u8> {
    let mut output = vec![1, sps[1], sps[2], sps[3], 0xff, 0xe1];
    output.extend((sps.len() as u16).to_be_bytes());
    output.extend(sps);
    output.push(1);
    output.extend((pps.len() as u16).to_be_bytes());
    output.extend(pps);
    output
}

fn info(_config: &StreamConfig) -> Vec<u8> {
    let mut body = Vec::new();
    body.extend(element(0x2ad7_b1, uint(TIME_CODE_SCALE_NS)));
    body.extend(element(0x4d80, b"TabDisplay".to_vec()));
    body.extend(element(0x5741, b"TabDisplay Matroska".to_vec()));
    element(0x1549_a966, body)
}

fn tracks(config: &StreamConfig, parameter_sets: &(Vec<u8>, Vec<u8>)) -> Vec<u8> {
    let mut entries = video_track(config, parameter_sets);
    if config.audio {
        entries.extend(audio_track());
    }
    element(0x1654_ae6b, entries)
}

fn video_track(config: &StreamConfig, parameter_sets: &(Vec<u8>, Vec<u8>)) -> Vec<u8> {
    let mut body = Vec::new();
    body.extend(element(0xd7, uint(1)));
    body.extend(element(0x73c5, uint(1)));
    body.extend(element(0x83, uint(1)));
    body.extend(element(0x86, b"V_MPEG4/ISO/AVC".to_vec()));
    body.extend(element(0x63a2, avc_codec_private(parameter_sets)));
    let mut video = Vec::new();
    video.extend(element(0xb0, uint(config.width as u64)));
    video.extend(element(0xba, uint(config.height as u64)));
    video.extend(element(0x2383_e3, float(config.fps as f64)));
    body.extend(element(0xe0, video));
    element(0xae, body)
}

fn audio_track() -> Vec<u8> {
    let mut body = Vec::new();
    body.extend(element(0xd7, uint(2)));
    body.extend(element(0x73c5, uint(2)));
    body.extend(element(0x83, uint(2)));
    body.extend(element(0x86, b"A_OPUS".to_vec()));
    body.extend(element(
        0x63a2,
        b"OpusHead\x01\x02\x00\x00\x80\xbb\x00\x00\x00\x00\x00".to_vec(),
    ));
    let mut audio = Vec::new();
    audio.extend(element(0xb5, float(48_000.0)));
    audio.extend(element(0x9f, uint(2)));
    body.extend(element(0xe1, audio));
    element(0xae, body)
}

fn clusters(ordered: &[(u64, u8, &Packet)]) -> Result<Vec<u8>, MuxError> {
    let mut output = Vec::new();
    let mut cluster_base = None;
    let mut cluster_body = Vec::new();
    for (pts_ms, track, packet) in ordered {
        if cluster_base.is_none() {
            cluster_base = Some(*pts_ms);
            cluster_body.extend(element(0xe7, uint(*pts_ms)));
        }
        if pts_ms.saturating_sub(cluster_base.unwrap()) > CLUSTER_SPAN_MS {
            output.extend(element(0x1f43_b675, cluster_body));
            cluster_base = Some(*pts_ms);
            cluster_body = element(0xe7, uint(*pts_ms));
        }
        let relative = pts_ms
            .checked_sub(cluster_base.unwrap())
            .ok_or(MuxError::TimestampOutOfRange)?;
        if relative > i16::MAX as u64 {
            return Err(MuxError::TimestampOutOfRange);
        }
        let mut block = vec![0x80 | *track, (relative >> 8) as u8, relative as u8];
        match packet {
            Packet::Video { keyframe, data, .. } => {
                block.push(if *keyframe { 0x80 } else { 0 });
                block.extend(avc_access_unit(data)?);
            }
            Packet::Audio { data, .. } => {
                block.push(0);
                block.extend(data);
            }
        }
        cluster_body.extend(element(0xa3, block));
    }
    if !cluster_body.is_empty() {
        output.extend(element(0x1f43_b675, cluster_body));
    }
    Ok(output)
}

fn ebml_header() -> Vec<u8> {
    let mut body = Vec::new();
    body.extend(element(0x4286, uint(1)));
    body.extend(element(0x42f7, uint(1)));
    body.extend(element(0x42f2, uint(4)));
    body.extend(element(0x42f3, uint(8)));
    body.extend(element(0x4282, b"matroska".to_vec()));
    body.extend(element(0x4287, uint(4)));
    body.extend(element(0x4285, uint(2)));
    element(0x1a45_dfa3, body)
}

fn element(id: u32, content: Vec<u8>) -> Vec<u8> {
    let mut output = id_bytes(id);
    output.extend(vint_size(content.len()).expect("Matroska element is bounded"));
    output.extend(content);
    output
}

fn id_bytes(id: u32) -> Vec<u8> {
    let bytes = id.to_be_bytes();
    let start = bytes.iter().position(|byte| *byte != 0).unwrap_or(3);
    bytes[start..].to_vec()
}

fn vint_size(size: usize) -> Option<Vec<u8>> {
    let size = size as u64;
    for width in 1..=8 {
        let max = (1u64 << (7 * width)) - 2;
        if size <= max {
            let mut value = size | (1u64 << (7 * width));
            let mut output = vec![0; width];
            for byte in output.iter_mut().rev() {
                *byte = value as u8;
                value >>= 8;
            }
            return Some(output);
        }
    }
    None
}

fn unknown_size() -> Vec<u8> {
    vec![0x01, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff]
}

fn uint(value: u64) -> Vec<u8> {
    let bytes = value.to_be_bytes();
    let start = bytes.iter().position(|byte| *byte != 0).unwrap_or(7);
    bytes[start..].to_vec()
}

fn float(value: f64) -> Vec<u8> {
    value.to_be_bytes().to_vec()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config() -> StreamConfig {
        StreamConfig {
            generation: 1,
            width: 1152,
            height: 640,
            fps: 60,
            audio: true,
        }
    }

    fn keyframe() -> Vec<u8> {
        vec![
            0, 0, 0, 1, 0x67, 0x42, 0, 0x1f, // SPS
            0, 0, 1, 0x68, 0xce, 6, 0xe2, // PPS
            0, 0, 0, 1, 0x65, 0x88, 0x84, // IDR
        ]
    }

    #[test]
    fn encodes_avc_opus_tracks_and_timestamped_clusters() {
        let packets = vec![
            Packet::Video {
                pts_ms: 0,
                keyframe: true,
                data: keyframe(),
            },
            Packet::Audio {
                pts_samples: 0,
                data: vec![0xf8, 0xff, 0xfe],
            },
            Packet::Video {
                pts_ms: 33,
                keyframe: false,
                data: vec![0, 0, 1, 0x41, 0x9a],
            },
        ];
        let output = encode_segment(&config(), &packets).unwrap();
        assert_eq!(&output[..4], &[0x1a, 0x45, 0xdf, 0xa3]);
        assert!(output.windows(8).any(|window| window == b"matroska"));
        assert!(output
            .windows(4)
            .any(|window| window == [0x16, 0x54, 0xae, 0x6b]));
        assert!(output
            .windows(4)
            .any(|window| window == [0x1f, 0x43, 0xb6, 0x75]));
        assert!(output.contains(&0xa3));
        assert!(output.windows(8).any(|window| window == b"OpusHead"));
        assert!(output.windows(4).any(|window| window == [0, 0, 0, 3]));
    }

    #[test]
    fn rejects_missing_idr_parameter_sets_and_invalid_timestamps() {
        let non_idr = [Packet::Video {
            pts_ms: 0,
            keyframe: false,
            data: vec![0, 0, 1, 0x41, 0x01],
        }];
        assert_eq!(
            encode_segment(&config(), &non_idr),
            Err(MuxError::MissingKeyframe)
        );

        let no_sps = [Packet::Video {
            pts_ms: 0,
            keyframe: true,
            data: vec![0, 0, 1, 0x65, 0x01],
        }];
        assert_eq!(
            encode_segment(&config(), &no_sps),
            Err(MuxError::MissingParameterSets)
        );

        let packets = [
            Packet::Video {
                pts_ms: 10,
                keyframe: true,
                data: keyframe(),
            },
            Packet::Video {
                pts_ms: 9,
                keyframe: false,
                data: vec![0, 0, 1, 0x41, 0x01],
            },
        ];
        assert_eq!(
            encode_segment(&config(), &packets),
            Err(MuxError::NonMonotonicTimestamp)
        );
    }

    #[test]
    fn rejects_a_segment_larger_than_the_bounded_queue() {
        let mut packets = vec![Packet::Video {
            pts_ms: 0,
            keyframe: true,
            data: keyframe(),
        }];
        for sequence in 0..=(MAX_QUEUE_BYTES / MAX_PACKET_BYTES) {
            packets.push(Packet::Audio {
                pts_samples: (sequence as u64 + 1) * 960,
                data: vec![0; MAX_PACKET_BYTES],
            });
        }
        assert_eq!(
            encode_segment(&config(), &packets),
            Err(MuxError::QueueTooLarge)
        );
    }
}
