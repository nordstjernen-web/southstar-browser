//! Southstar — the H.264 and AAC configuration and packet reshaping the Media Foundation decoders need: avcC to Annex B, and AudioSpecificConfig parsing.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

const START_CODE: [u8; 4] = [0, 0, 0, 1];
const AAC_RATES: [u32; 13] = [
    96000, 88200, 64000, 48000, 44100, 32000, 24000, 22050, 16000, 12000, 11025, 8000, 7350,
];
const AOT_AAC_LC: u8 = 2;
const AOT_SBR: u8 = 5;
const AOT_PS: u8 = 29;

#[derive(Debug, PartialEq, Eq)]
pub struct H264Config {
    pub parameter_sets: Vec<u8>,
    pub nal_length_size: usize,
}

pub fn h264_config(extradata: &[u8]) -> Option<H264Config> {
    if extradata.first() != Some(&1) {
        return Some(H264Config {
            parameter_sets: extradata.to_vec(),
            nal_length_size: 0,
        });
    }
    let nal_length_size = usize::from(*extradata.get(4)? & 3) + 1;
    let mut parameter_sets = Vec::new();
    let mut at = 5;
    let sps_count = *extradata.get(at)? & 0x1f;
    at += 1;
    at = copy_sets(extradata, at, usize::from(sps_count), &mut parameter_sets)?;
    let pps_count = *extradata.get(at)?;
    at += 1;
    copy_sets(extradata, at, usize::from(pps_count), &mut parameter_sets)?;
    Some(H264Config {
        parameter_sets,
        nal_length_size,
    })
}

fn copy_sets(data: &[u8], mut at: usize, count: usize, out: &mut Vec<u8>) -> Option<usize> {
    for _ in 0..count {
        let length = usize::from(u16::from_be_bytes([*data.get(at)?, *data.get(at + 1)?]));
        at += 2;
        let set = data.get(at..at + length)?;
        out.extend_from_slice(&START_CODE);
        out.extend_from_slice(set);
        at += length;
    }
    Some(at)
}

pub fn to_annex_b(packet: &[u8], nal_length_size: usize, out: &mut Vec<u8>) -> bool {
    out.clear();
    if nal_length_size == 0 {
        out.extend_from_slice(packet);
        return true;
    }
    let mut at = 0;
    while at < packet.len() {
        let Some(prefix) = packet.get(at..at + nal_length_size) else {
            return false;
        };
        let length = prefix
            .iter()
            .fold(0usize, |length, &byte| (length << 8) | usize::from(byte));
        at += nal_length_size;
        let Some(unit) = packet.get(at..at + length) else {
            return false;
        };
        out.extend_from_slice(&START_CODE);
        out.extend_from_slice(unit);
        at += length;
    }
    true
}

#[derive(Debug, PartialEq, Eq)]
pub struct AacConfig {
    pub object_type: u8,
    pub rate: u32,
    pub channels: u32,
    pub rate_index: Option<u8>,
}

struct Bits<'a> {
    data: &'a [u8],
    at: usize,
}

impl Bits<'_> {
    fn read(&mut self, count: usize) -> Option<u32> {
        let mut value = 0;
        for _ in 0..count {
            let byte = *self.data.get(self.at / 8)?;
            let bit = (byte >> (7 - self.at % 8)) & 1;
            value = (value << 1) | u32::from(bit);
            self.at += 1;
        }
        Some(value)
    }
}

pub fn aac_config(config: &[u8]) -> Option<AacConfig> {
    let mut bits = Bits {
        data: config,
        at: 0,
    };
    let mut object_type = bits.read(5)?;
    if object_type == 31 {
        object_type = 32 + bits.read(6)?;
    }
    let index = bits.read(4)?;
    let (rate, rate_index) = if index == 15 {
        (bits.read(24)?, None)
    } else {
        let rate = *AAC_RATES.get(index as usize)?;
        (rate, Some(index as u8))
    };
    let channels = bits.read(4)?;
    Some(AacConfig {
        object_type: u8::try_from(object_type).ok()?,
        rate,
        channels,
        rate_index,
    })
}

pub fn aac_core_config(config: &[u8]) -> Option<(Vec<u8>, AacConfig)> {
    let parsed = aac_config(config)?;
    let is_sbr = parsed.object_type == AOT_SBR || parsed.object_type == AOT_PS;
    match parsed.rate_index {
        Some(index) if is_sbr && (1..=7).contains(&parsed.channels) => {
            let channels = parsed.channels as u8;
            let core = vec![
                (AOT_AAC_LC << 3) | (index >> 1),
                ((index & 1) << 7) | (channels << 3),
            ];
            let core_config = AacConfig {
                object_type: AOT_AAC_LC,
                ..parsed
            };
            Some((core, core_config))
        }
        _ => Some((config.to_vec(), parsed)),
    }
}
