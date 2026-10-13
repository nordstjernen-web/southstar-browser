//! Southstar — the polyphase synthesis filterbank turning 32 subbands into PCM samples. Ported from minimp3 by lieff (CC0).
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use crate::tables::{SEC, WIN};

const SCALE: f32 = 1.0 / 32768.0;

fn dct_ii(grbuf: &mut [f32], n: usize) {
    for k in 0..n {
        let mut t = [[0.0f32; 8]; 4];
        for i in 0..8 {
            let x0 = grbuf[k + i * 18];
            let x1 = grbuf[k + (15 - i) * 18];
            let x2 = grbuf[k + (16 + i) * 18];
            let x3 = grbuf[k + (31 - i) * 18];
            let t0 = x0 + x3;
            let t1 = x1 + x2;
            let t2 = (x1 - x2) * SEC[3 * i];
            let t3 = (x0 - x3) * SEC[3 * i + 1];
            t[0][i] = t0 + t1;
            t[1][i] = (t0 - t1) * SEC[3 * i + 2];
            t[2][i] = t3 + t2;
            t[3][i] = (t3 - t2) * SEC[3 * i + 2];
        }
        for x in t.iter_mut() {
            let (mut x0, mut x1, mut x2, mut x3) = (x[0], x[1], x[2], x[3]);
            let (mut x4, mut x5, mut x6, mut x7) = (x[4], x[5], x[6], x[7]);
            let mut xt = x0 - x7;
            x0 += x7;
            x7 = x1 - x6;
            x1 += x6;
            x6 = x2 - x5;
            x2 += x5;
            x5 = x3 - x4;
            x3 += x4;
            x4 = x0 - x3;
            x0 += x3;
            x3 = x1 - x2;
            x1 += x2;
            x[0] = x0 + x1;
            x[4] = (x0 - x1) * 0.707_106_77;
            x5 += x6;
            x6 = (x6 + x7) * 0.707_106_77;
            x7 += xt;
            x3 = (x3 + x4) * 0.707_106_77;
            x5 -= x7 * 0.198_912_367;
            x7 += x5 * 0.382_683_432;
            x5 -= x7 * 0.198_912_367;
            x0 = xt - x6;
            xt += x6;
            x[1] = (xt + x7) * 0.509_795_61;
            x[2] = (x4 + x3) * 0.541_196_11;
            x[3] = (x0 - x5) * 0.601_344_88;
            x[5] = (x0 + x5) * 0.899_976_19;
            x[6] = (x4 - x3) * 1.306_563_02;
            x[7] = (xt - x7) * 2.562_915_56;
        }
        let mut y = k;
        for i in 0..7 {
            let s = t[3][i] + t[3][i + 1];
            grbuf[y] = t[0][i];
            grbuf[y + 18] = t[2][i] + s;
            grbuf[y + 2 * 18] = t[1][i] + t[1][i + 1];
            grbuf[y + 3 * 18] = t[2][i + 1] + s;
            y += 4 * 18;
        }
        grbuf[y] = t[0][7];
        grbuf[y + 18] = t[2][7] + t[3][7];
        grbuf[y + 2 * 18] = t[1][7];
        grbuf[y + 3 * 18] = t[3][7];
    }
}

fn synth_pair(pcm: &mut [f32], at: usize, nch: usize, lins: &[f32], zb: usize) {
    let z = |k: usize| lins[zb + k];
    let mut a = (z(14 * 64) - z(0)) * 29.0;
    a += (z(64) + z(13 * 64)) * 213.0;
    a += (z(12 * 64) - z(2 * 64)) * 459.0;
    a += (z(3 * 64) + z(11 * 64)) * 2037.0;
    a += (z(10 * 64) - z(4 * 64)) * 5153.0;
    a += (z(5 * 64) + z(9 * 64)) * 6574.0;
    a += (z(8 * 64) - z(6 * 64)) * 37489.0;
    a += z(7 * 64) * 75038.0;
    pcm[at] = a * SCALE;

    let z = |k: usize| lins[zb + 2 + k];
    a = z(14 * 64) * 104.0;
    a += z(12 * 64) * 1567.0;
    a += z(10 * 64) * 9727.0;
    a += z(8 * 64) * 64019.0;
    a += z(6 * 64) * -9975.0;
    a += z(4 * 64) * -45.0;
    a += z(2 * 64) * 146.0;
    a += z(0) * -5.0;
    pcm[at + 16 * nch] = a * SCALE;
}

fn synth(
    grbuf: &[f32],
    xl: usize,
    pcm: &mut [f32],
    dst: usize,
    nch: usize,
    lins: &mut [f32],
    lb: usize,
) {
    let xr = xl + 576 * (nch - 1);
    let dstl = dst;
    let dstr = dst + nch - 1;
    let zlin = lb + 15 * 64;

    lins[zlin + 4 * 15] = grbuf[xl + 18 * 16];
    lins[zlin + 4 * 15 + 1] = grbuf[xr + 18 * 16];
    lins[zlin + 4 * 15 + 2] = grbuf[xl];
    lins[zlin + 4 * 15 + 3] = grbuf[xr];

    lins[zlin + 4 * 31] = grbuf[xl + 1 + 18 * 16];
    lins[zlin + 4 * 31 + 1] = grbuf[xr + 1 + 18 * 16];
    lins[zlin + 4 * 31 + 2] = grbuf[xl + 1];
    lins[zlin + 4 * 31 + 3] = grbuf[xr + 1];

    synth_pair(pcm, dstr, nch, lins, lb + 4 * 15 + 1);
    synth_pair(pcm, dstr + 32 * nch, nch, lins, lb + 4 * 15 + 64 + 1);
    synth_pair(pcm, dstl, nch, lins, lb + 4 * 15);
    synth_pair(pcm, dstl + 32 * nch, nch, lins, lb + 4 * 15 + 64);

    for i in (0..15).rev() {
        let zi = zlin + 4 * i;
        lins[zi] = grbuf[xl + 18 * (31 - i)];
        lins[zi + 1] = grbuf[xr + 18 * (31 - i)];
        lins[zi + 2] = grbuf[xl + 1 + 18 * (31 - i)];
        lins[zi + 3] = grbuf[xr + 1 + 18 * (31 - i)];
        lins[zi + 64] = grbuf[xl + 1 + 18 * (1 + i)];
        lins[zi + 64 + 1] = grbuf[xr + 1 + 18 * (1 + i)];
        lins[zi - 64 + 2] = grbuf[xl + 18 * (1 + i)];
        lins[zi - 64 + 3] = grbuf[xr + 18 * (1 + i)];

        let mut a = [0.0f32; 4];
        let mut b = [0.0f32; 4];
        let w = (14 - i) * 16;
        for k in 0..8 {
            let w0 = WIN[w + 2 * k];
            let w1 = WIN[w + 2 * k + 1];
            let vz = zi - 64 * k;
            let vy = zi - 64 * (15 - k);
            for j in 0..4 {
                let z = lins[vz + j];
                let y = lins[vy + j];
                let bk = z * w1 + y * w0;
                if k == 0 {
                    b[j] = bk;
                    a[j] = z * w0 - y * w1;
                } else if k % 2 == 1 {
                    b[j] += bk;
                    a[j] += y * w1 - z * w0;
                } else {
                    b[j] += bk;
                    a[j] += z * w0 - y * w1;
                }
            }
        }

        pcm[dstr + (15 - i) * nch] = a[1] * SCALE;
        pcm[dstr + (17 + i) * nch] = b[1] * SCALE;
        pcm[dstl + (15 - i) * nch] = a[0] * SCALE;
        pcm[dstl + (17 + i) * nch] = b[0] * SCALE;
        pcm[dstr + (47 - i) * nch] = a[3] * SCALE;
        pcm[dstr + (49 + i) * nch] = b[3] * SCALE;
        pcm[dstl + (47 - i) * nch] = a[2] * SCALE;
        pcm[dstl + (49 + i) * nch] = b[2] * SCALE;
    }
}

pub fn synth_granule(
    qmf_state: &mut [f32; 960],
    grbuf: &mut [f32],
    nbands: usize,
    nch: usize,
    pcm: &mut [f32],
    pcm_offset: usize,
    lins: &mut [f32],
) {
    for i in 0..nch {
        dct_ii(&mut grbuf[576 * i..576 * i + 576], nbands);
    }
    lins[..960].copy_from_slice(qmf_state);
    for i in (0..nbands).step_by(2) {
        synth(grbuf, i, pcm, pcm_offset + 32 * nch * i, nch, lins, i * 64);
    }
    let tail = &lins[nbands * 64..nbands * 64 + 960];
    if nch == 1 {
        for i in (0..960).step_by(2) {
            qmf_state[i] = tail[i];
        }
    } else {
        qmf_state.copy_from_slice(tail);
    }
}
