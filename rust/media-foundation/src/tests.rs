//! Southstar — unit tests for the avcC and AudioSpecificConfig handling the Media Foundation decoders rely on.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use crate::bitstream::{AacConfig, aac_config, aac_core_config, h264_config, to_annex_b};

#[test]
fn avcc_parameter_sets_become_annex_b() {
    let avcc = [
        1, 0x64, 0, 0x1f, 0xff, 0xe1, 0, 3, 0x67, 0xaa, 0xbb, 1, 0, 2, 0x68, 0xcc,
    ];
    let config = h264_config(&avcc).expect("valid avcC");
    assert_eq!(config.nal_length_size, 4);
    assert_eq!(
        config.parameter_sets,
        [0, 0, 0, 1, 0x67, 0xaa, 0xbb, 0, 0, 0, 1, 0x68, 0xcc]
    );
}

#[test]
fn truncated_avcc_is_rejected() {
    assert_eq!(
        h264_config(&[1, 0x64, 0, 0x1f, 0xff, 0xe1, 0, 9, 0x67]),
        None
    );
}

#[test]
fn annex_b_extradata_passes_through() {
    let config = h264_config(&[0, 0, 0, 1, 0x67]).expect("annex b");
    assert_eq!(config.nal_length_size, 0);
    assert_eq!(config.parameter_sets, [0, 0, 0, 1, 0x67]);
}

#[test]
fn length_prefixed_packets_become_annex_b() {
    let mut out = Vec::new();
    assert!(to_annex_b(&[0, 2, 0x65, 0x88, 0, 1, 0x06], 2, &mut out));
    assert_eq!(out, [0, 0, 0, 1, 0x65, 0x88, 0, 0, 0, 1, 0x06]);
    assert!(!to_annex_b(&[0, 9, 0x65], 2, &mut out));
    assert!(to_annex_b(&[0, 0, 0, 1, 0x65], 0, &mut out));
    assert_eq!(out, [0, 0, 0, 1, 0x65]);
}

#[test]
fn aac_lc_config_parses() {
    assert_eq!(
        aac_config(&[0x12, 0x10]),
        Some(AacConfig {
            object_type: 2,
            rate: 44100,
            channels: 2,
            rate_index: Some(4),
        })
    );
}

#[test]
fn he_aac_config_is_rewritten_to_its_lc_core() {
    let (core, parsed) = aac_core_config(&[0x2b, 0x92, 0x08, 0x00]).expect("HE-AAC");
    assert_eq!(parsed.object_type, 2);
    assert_eq!(parsed.rate, 22050);
    assert_eq!(parsed.channels, 2);
    assert_eq!(core, [0x13, 0x90]);
    assert_eq!(
        aac_config(&core).map(|c| (c.object_type, c.rate)),
        Some((2, 22050))
    );
}

#[test]
fn lc_config_is_kept() {
    let (core, _) = aac_core_config(&[0x12, 0x10]).expect("LC");
    assert_eq!(core, [0x12, 0x10]);
}
