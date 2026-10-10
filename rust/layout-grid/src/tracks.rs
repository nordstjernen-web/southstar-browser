//! Southstar — grid track sizing: fixed, percentage, flexible, intrinsic, minmax() and fit-content() tracks, repeat(auto-fill/auto-fit) expansion and spanning-item distribution.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use southstar_css::{
    AUTO_REPEAT_FIT, AUTO_REPEAT_NONE, GridLineName, GridTrack, GridTracks, LINE_NAMES_MAX,
    TRACK_AUTO, TRACK_FR, TRACK_MAX_CONTENT, TRACK_MIN_CONTENT, TRACK_PERCENT, TRACK_PX,
    TRACKS_MAX,
};

use crate::text::{fmax, fmin};

pub(crate) const MAX: usize = TRACKS_MAX;

pub(crate) fn count(tracks: &GridTracks) -> usize {
    usize::try_from(tracks.n).unwrap_or(0).min(MAX)
}

pub(crate) fn auto_track() -> GridTrack {
    GridTrack {
        kind: TRACK_AUTO,
        ..GridTrack::default()
    }
}

pub(crate) fn single_auto() -> GridTracks {
    let mut t = GridTracks {
        n: 1,
        ..GridTracks::default()
    };
    t.tracks[0] = auto_track();
    t
}

pub(crate) fn track_min_px(t: &GridTrack, avail: f64) -> f64 {
    if t.has_min == 0 {
        return 0.0;
    }
    match t.min_kind {
        TRACK_PX => t.min_v + t.min_pct * avail / 100.0,
        TRACK_PERCENT => t.min_v * avail / 100.0,
        _ => 0.0,
    }
}

pub(crate) fn is_intrinsic(kind: u32) -> bool {
    kind == TRACK_AUTO || kind == TRACK_MIN_CONTENT || kind == TRACK_MAX_CONTENT
}

fn flex_track_sizes(tr: &GridTracks, space: f64, available_main: f64, sizes: &mut [f64]) -> f64 {
    let n = count(tr);
    let mut inflexible = [false; MAX];
    let mut base = [0.0f64; MAX];
    for (b, t) in base.iter_mut().zip(&tr.tracks[..n]) {
        if t.kind == TRACK_FR {
            *b = track_min_px(t, available_main);
        }
    }
    let mut fr = 0.0;
    for _ in 0..=n {
        let mut leftover = space;
        let mut flex_sum = 0.0;
        for i in 0..n {
            if tr.tracks[i].kind != TRACK_FR {
                continue;
            }
            if inflexible[i] {
                leftover -= base[i];
            } else {
                flex_sum += fmax(tr.tracks[i].v, 0.0);
            }
        }
        fr = if flex_sum > 0.0 {
            leftover / fmax(flex_sum, 1.0)
        } else {
            0.0
        };
        let mut changed = false;
        for i in 0..n {
            if tr.tracks[i].kind != TRACK_FR || inflexible[i] {
                continue;
            }
            if fr * fmax(tr.tracks[i].v, 0.0) < base[i] {
                inflexible[i] = true;
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }
    let mut used = 0.0;
    for i in 0..n {
        if tr.tracks[i].kind != TRACK_FR {
            continue;
        }
        sizes[i] = if inflexible[i] {
            base[i]
        } else {
            fmax(base[i], fr * fmax(tr.tracks[i].v, 0.0))
        };
        used += sizes[i];
    }
    used
}

pub(crate) fn resolve_sizes(
    tr: &GridTracks,
    available_main: f64,
    content: Option<(&[f64; MAX], &[f64; MAX])>,
    out_sizes: &mut [f64; MAX],
    stretch_auto: bool,
) {
    let n = count(tr);
    let content_min = content.map(|c| c.0);
    let content_max = content.map(|c| c.1);
    let mut total_fixed = 0.0;
    let mut total_fr = 0.0;
    let mut total_shrink = 0.0;
    let mut fixed_px = [0.0f64; MAX];
    let mut shrink_px = [0.0f64; MAX];
    let mut n_auto = 0;
    for i in 0..n {
        let t = &tr.tracks[i];
        let mut fixed = 0.0;
        match t.kind {
            TRACK_PX | TRACK_PERCENT => {
                fixed = if t.kind == TRACK_PX {
                    t.v + t.pct * available_main / 100.0
                } else {
                    t.v * available_main / 100.0
                };
                if t.fit_content != 0 {
                    fixed = fmin(fixed, content_max.map_or(0.0, |c| c[i]));
                }
                if t.has_min != 0
                    && is_intrinsic(t.min_kind)
                    && let Some(cmin) = content_min
                    && cmin[i] > fixed
                {
                    fixed = cmin[i];
                }
                fixed_px[i] = fixed;
                total_fixed += fixed;
            }
            TRACK_FR => total_fr += if t.v > 0.0 { t.v } else { 0.0 },
            TRACK_AUTO => n_auto += 1,
            _ => {}
        }
        if (t.kind == TRACK_PX || t.kind == TRACK_PERCENT) && t.has_min != 0 {
            let mn = track_min_px(t, available_main);
            if fixed > mn {
                shrink_px[i] = fixed - mn;
                total_shrink += shrink_px[i];
            }
        }
    }
    let mut auto_base = [0.0f64; MAX];
    let mut auto_lim = [0.0f64; MAX];
    if let Some(cmin) = content_min {
        let mut base_sum = 0.0;
        let mut auto_sum = 0.0;
        let mut content_sum = 0.0;
        for i in 0..n {
            if !is_intrinsic(tr.tracks[i].kind) {
                continue;
            }
            auto_base[i] = if cmin[i] > 0.0 { cmin[i] } else { 0.0 };
            let lim = content_max.map_or(auto_base[i], |c| c[i]);
            auto_lim[i] = if lim > auto_base[i] {
                lim
            } else {
                auto_base[i]
            };
            if tr.tracks[i].kind == TRACK_MAX_CONTENT {
                auto_base[i] = auto_lim[i];
            }
            base_sum += auto_base[i];
            if tr.tracks[i].kind == TRACK_AUTO {
                auto_sum += auto_base[i];
            } else {
                content_sum += auto_base[i];
            }
        }
        let mut free_for_auto = available_main - (total_fixed - total_shrink);
        if free_for_auto < 0.0 {
            free_for_auto = 0.0;
        }
        let mut room = free_for_auto - content_sum;
        if room < 0.0 {
            room = 0.0;
        }
        if auto_sum > room && auto_sum > 0.0 {
            let scale = room / auto_sum;
            for i in 0..n {
                if tr.tracks[i].kind != TRACK_AUTO {
                    continue;
                }
                auto_base[i] *= scale;
                if auto_lim[i] < auto_base[i] {
                    auto_lim[i] = auto_base[i];
                }
            }
            base_sum = content_sum + room;
        }
        total_fixed += base_sum;
    }

    let mut fr_min_total = 0.0;
    for t in &tr.tracks[..n] {
        if t.kind == TRACK_FR {
            fr_min_total += track_min_px(t, available_main);
        }
    }

    let mut shrink_used = 0.0;
    if total_fixed + fr_min_total > available_main && total_shrink > 0.0 {
        shrink_used = total_fixed + fr_min_total - available_main;
        if shrink_used > total_shrink {
            shrink_used = total_shrink;
        }
        total_fixed -= shrink_used;
    }

    let mut remaining = available_main - total_fixed;
    if remaining < 0.0 {
        remaining = 0.0;
    }

    let mut auto_grow = [0.0f64; MAX];
    if n_auto > 0 && remaining > fr_min_total && content_min.is_some() {
        let mut room_total = 0.0;
        for i in 0..n {
            if tr.tracks[i].kind != TRACK_AUTO {
                continue;
            }
            let room = auto_lim[i] - auto_base[i];
            if room > 0.0 {
                room_total += room;
            }
        }
        if room_total > 0.0 {
            let free_space = remaining - fr_min_total;
            let give = if free_space < room_total {
                free_space
            } else {
                room_total
            };
            for i in 0..n {
                if tr.tracks[i].kind != TRACK_AUTO {
                    continue;
                }
                let room = auto_lim[i] - auto_base[i];
                if room > 0.0 {
                    auto_grow[i] = give * (room / room_total);
                }
            }
            remaining -= give;
        }
    }

    let mut fr_sizes = [0.0f64; MAX];
    let fr_used = if total_fr > 0.0 {
        flex_track_sizes(tr, remaining, available_main, &mut fr_sizes)
    } else {
        0.0
    };
    let mut per_auto = 0.0;
    if n_auto > 0 && stretch_auto && remaining - fr_used > 0.0 {
        per_auto = (remaining - fr_used) / f64::from(n_auto);
    }

    for i in 0..n {
        let t = &tr.tracks[i];
        match t.kind {
            TRACK_PX | TRACK_PERCENT => {
                out_sizes[i] = fixed_px[i];
                if shrink_used > 0.0 && shrink_px[i] > 0.0 {
                    out_sizes[i] -= shrink_used * (shrink_px[i] / total_shrink);
                }
            }
            TRACK_FR => out_sizes[i] = fr_sizes[i],
            TRACK_AUTO => out_sizes[i] = auto_base[i] + auto_grow[i] + per_auto,
            TRACK_MIN_CONTENT => out_sizes[i] = auto_base[i],
            TRACK_MAX_CONTENT => {
                out_sizes[i] = if auto_lim[i] > auto_base[i] {
                    auto_lim[i]
                } else {
                    auto_base[i]
                }
            }
            _ => {}
        }
        let mn = track_min_px(t, available_main);
        if out_sizes[i] < mn {
            out_sizes[i] = mn;
        }
        if out_sizes[i] < 0.0 {
            out_sizes[i] = 0.0;
        }
    }
}

fn track_repeat_px(t: &GridTrack, available_main: f64) -> f64 {
    let min_px = track_min_px(t, available_main);
    if t.kind == TRACK_PX || t.kind == TRACK_PERCENT {
        let max_px = if t.kind == TRACK_PX {
            t.v + t.pct * available_main / 100.0
        } else {
            t.v * available_main / 100.0
        };
        return if max_px > min_px { max_px } else { min_px };
    }
    min_px
}

fn line_name_copy(out: &mut GridTracks, ln: &GridLineName, line: i32) {
    let at = usize::try_from(out.n_line_names).unwrap_or(LINE_NAMES_MAX);
    if at >= LINE_NAMES_MAX {
        return;
    }
    out.line_names[at] = *ln;
    out.line_names[at].line = line;
    out.n_line_names += 1;
}

pub(crate) fn expand_repeat_names(tr: &GridTracks, repeats: i32, out: &mut GridTracks) {
    let first = tr.auto_repeat_names_start;
    let last = tr.auto_repeat_names_end;
    let shift = (repeats - 1) * tr.auto_repeat_count;
    let name = |i: i32| &tr.line_names[i as usize];
    out.n_line_names = 0;
    for i in 0..first {
        line_name_copy(out, name(i), name(i).line);
    }
    for r in 0..repeats {
        for i in first..last {
            line_name_copy(out, name(i), name(i).line + r * tr.auto_repeat_count);
        }
    }
    for i in last..tr.n_line_names {
        line_name_copy(out, name(i), name(i).line + shift);
    }
}

pub(crate) struct Expanded {
    pub tracks: GridTracks,
    pub fit_start: i32,
    pub fit_count: i32,
}

pub(crate) fn expand_auto_repeat(tr: &GridTracks, available_main: f64, gap: f64) -> Expanded {
    let mut out = Expanded {
        tracks: *tr,
        fit_start: 0,
        fit_count: 0,
    };
    if tr.auto_repeat == AUTO_REPEAT_NONE
        || tr.auto_repeat_count <= 0
        || tr.auto_repeat_start < 0
        || tr.auto_repeat_start >= tr.n
    {
        return out;
    }
    let clamped;
    let tr = if tr.auto_repeat_count > tr.n - tr.auto_repeat_start {
        let mut c = *tr;
        c.auto_repeat_count = tr.n - tr.auto_repeat_start;
        clamped = c;
        &clamped
    } else {
        tr
    };
    let start = tr.auto_repeat_start as usize;
    let rcount = tr.auto_repeat_count as usize;
    let n = tr.n as usize;

    let mut base_min = 0.0;
    for t in &tr.tracks[start..start + rcount] {
        let m = track_repeat_px(t, available_main);
        if m <= 0.0 {
            out.tracks = single_auto();
            return out;
        }
        base_min += m;
    }
    if base_min <= 0.0 {
        return out;
    }
    let mut others = 0.0;
    let n_others = tr.n - tr.auto_repeat_count;
    for (i, t) in tr.tracks[..n].iter().enumerate() {
        if i >= start && i < start + rcount {
            continue;
        }
        let m = track_repeat_px(t, available_main);
        if m > 0.0 {
            others += m;
        }
    }
    let pattern_with_gap = base_min + gap * f64::from(tr.auto_repeat_count);
    let room = available_main - others - f64::from(n_others - 1) * gap;
    let mut repeats = 1;
    if pattern_with_gap > 0.0 {
        repeats = (room / pattern_with_gap) as i32;
    }
    repeats = repeats.clamp(1, MAX as i32);

    let prefix = tr.auto_repeat_start;
    let suffix_start = tr.auto_repeat_start + tr.auto_repeat_count;
    let suffix_count = tr.n - suffix_start;
    let total = prefix + repeats * tr.auto_repeat_count + suffix_count;
    if total > MAX as i32 {
        repeats = (MAX as i32 - prefix - suffix_count) / tr.auto_repeat_count;
    }
    if repeats < 1 {
        repeats = 1;
    }

    let o = &mut out.tracks;
    o.n = 0;
    let push = |o: &mut GridTracks, t: GridTrack| {
        if (o.n as usize) < MAX {
            o.tracks[o.n as usize] = t;
            o.n += 1;
        }
    };
    for i in 0..prefix as usize {
        push(o, tr.tracks[i]);
    }
    for _ in 0..repeats {
        for i in 0..rcount {
            push(o, tr.tracks[start + i]);
        }
    }
    for i in 0..suffix_count.max(0) as usize {
        push(o, tr.tracks[suffix_start as usize + i]);
    }
    expand_repeat_names(tr, repeats, o);
    o.auto_repeat = AUTO_REPEAT_NONE;
    if tr.auto_repeat == AUTO_REPEAT_FIT {
        out.fit_start = prefix;
        out.fit_count = repeats * tr.auto_repeat_count;
    }
    out
}

fn row_below_limit(height: &[f64], limit: &[f64], k: usize) -> bool {
    limit[k] < 0.0 || height[k] < limit[k] - 0.01
}

fn spread_to_limits(height: &mut [f64], target: &[bool], limit: &[f64], mut extra: f64) -> f64 {
    let n = height.len();
    let mut round = 0;
    while round < n && extra > 0.01 {
        let open = (0..n)
            .filter(|&k| target[k] && row_below_limit(height, limit, k))
            .count();
        if open == 0 {
            break;
        }
        let share = extra / open as f64;
        for k in 0..n {
            if !target[k] || !row_below_limit(height, limit, k) {
                continue;
            }
            let add = if limit[k] < 0.0 {
                share
            } else {
                fmin(share, limit[k] - height[k])
            };
            height[k] += add;
            extra -= add;
        }
        round += 1;
    }
    extra
}

fn spread_beyond_limits(height: &mut [f64], target: &[bool], max_intrinsic: &[bool], extra: f64) {
    let n = height.len();
    let any_max = (0..n).any(|k| target[k] && max_intrinsic[k]);
    let beyond = (0..n)
        .filter(|&k| target[k] && (max_intrinsic[k] || !any_max))
        .count();
    if beyond == 0 {
        return;
    }
    for k in 0..n {
        if target[k] && (max_intrinsic[k] || !any_max) {
            height[k] += extra / beyond as f64;
        }
    }
}

pub(crate) struct RowSpan<'a> {
    pub fixed: &'a [bool],
    pub limit: &'a [f64],
    pub min_intrinsic: &'a [bool],
    pub max_intrinsic: &'a [bool],
}

pub(crate) fn distribute_span(
    height: &mut [f64],
    rows: &RowSpan<'_>,
    target: &mut Vec<bool>,
    extra: f64,
) {
    let n = height.len();
    let any_min = (0..n).any(|k| !rows.fixed[k] && rows.min_intrinsic[k]);
    target.clear();
    target.extend((0..n).map(|k| !rows.fixed[k] && (rows.min_intrinsic[k] || !any_min)));
    let extra = spread_to_limits(height, target, rows.limit, extra);
    if extra > 0.01 {
        spread_beyond_limits(height, target, rows.max_intrinsic, extra);
    }
}

pub(crate) fn track_px(t: Option<&GridTrack>, basis: f64) -> f64 {
    let Some(t) = t else {
        return 0.0;
    };
    if t.kind == TRACK_PX {
        return t.v
            + if basis >= 0.0 {
                t.pct * basis / 100.0
            } else {
                0.0
            };
    }
    if t.kind == TRACK_PERCENT && basis >= 0.0 {
        return t.v * basis / 100.0;
    }
    0.0
}

pub(crate) fn track_is_fixed(t: &GridTrack, basis: f64) -> bool {
    if t.kind == TRACK_PX {
        if t.pct != 0.0 && basis < 0.0 {
            return false;
        }
    } else if t.kind != TRACK_PERCENT || basis < 0.0 {
        return false;
    }
    if t.has_min == 0 {
        return true;
    }
    t.min_kind == TRACK_PX || (t.min_kind == TRACK_PERCENT && basis >= 0.0)
}

fn distribute_extra(
    sizes: &mut [f64; MAX],
    caps: &[f64; MAX],
    affected: &[bool; MAX],
    beyond: &[bool; MAX],
    count: usize,
    mut extra: f64,
) {
    let mut grow = [0.0f64; MAX];
    let mut frozen = [false; MAX];
    for i in 0..count {
        frozen[i] = !affected[i];
    }
    while extra > 1e-9 {
        let open = frozen[..count].iter().filter(|&&f| !f).count();
        if open == 0 {
            break;
        }
        let share = extra / open as f64;
        let mut capped = false;
        for i in 0..count {
            if frozen[i] {
                continue;
            }
            let mut room = caps[i] - (sizes[i] + grow[i]);
            if room > share + 1e-9 {
                continue;
            }
            if room < 0.0 {
                room = 0.0;
            }
            grow[i] += room;
            extra -= room;
            frozen[i] = true;
            capped = true;
        }
        if capped {
            continue;
        }
        for i in 0..count {
            if !frozen[i] {
                grow[i] += share;
            }
        }
        extra = 0.0;
    }
    if extra > 1e-9 {
        let mut n = (0..count).filter(|&i| affected[i] && beyond[i]).count();
        let only_beyond = n > 0;
        if !only_beyond {
            n += (0..count).filter(|&i| affected[i]).count();
        }
        if n > 0 {
            for i in 0..count {
                if affected[i] && (!only_beyond || beyond[i]) {
                    grow[i] += extra / n as f64;
                }
            }
        }
    }
    for i in 0..count {
        sizes[i] += grow[i];
    }
}

fn max_is_intrinsic(t: &GridTrack) -> bool {
    is_intrinsic(t.kind)
}

fn min_is_intrinsic(t: &GridTrack) -> bool {
    if t.has_min != 0 {
        is_intrinsic(t.min_kind)
    } else {
        is_intrinsic(t.kind)
    }
}

fn fixed_px(t: &GridTrack, avail: f64) -> f64 {
    match t.kind {
        TRACK_PX => t.v + t.pct * avail / 100.0,
        TRACK_PERCENT => t.v * avail / 100.0,
        _ => 0.0,
    }
}

pub(crate) struct Contribution {
    pub min: f64,
    pub max: f64,
}

pub(crate) struct ColumnContent {
    pub min: [f64; MAX],
    pub max: [f64; MAX],
}

pub(crate) fn span_accommodate(
    cols: &GridTracks,
    c0: usize,
    span: usize,
    gap_after: &[f64],
    avail: f64,
    contribution: Contribution,
    content: &mut ColumnContent,
) -> bool {
    let ColumnContent {
        min: col_min,
        max: col_content,
    } = content;
    let mut gaps = 0.0;
    for i in 0..span {
        let t = &cols.tracks[c0 + i];
        if t.kind == TRACK_FR {
            return false;
        }
        if i + 1 < span {
            gaps += gap_after[c0 + i];
        }
    }
    let mut base = [0.0f64; MAX];
    let mut limit = [0.0f64; MAX];
    let mut caps = [0.0f64; MAX];
    let mut affected = [false; MAX];
    let mut beyond = [false; MAX];
    let mut any = false;
    let mut sum = gaps;
    for i in 0..span {
        let t = &cols.tracks[c0 + i];
        let min_intrinsic = min_is_intrinsic(t);
        let max_intrinsic = max_is_intrinsic(t);
        base[i] = if min_intrinsic {
            col_min[c0 + i]
        } else if t.has_min != 0 {
            track_min_px(t, avail)
        } else {
            fixed_px(t, avail)
        };
        caps[i] = if max_intrinsic {
            f64::INFINITY
        } else {
            fixed_px(t, avail)
        };
        affected[i] = min_intrinsic;
        beyond[i] = max_intrinsic;
        any = any || min_intrinsic || max_intrinsic;
        sum += base[i];
    }
    if !any {
        return false;
    }
    if contribution.min > sum {
        distribute_extra(
            &mut base,
            &caps,
            &affected,
            &beyond,
            span,
            contribution.min - sum,
        );
        for i in 0..span {
            if affected[i] {
                col_min[c0 + i] = base[i];
            }
        }
    }
    sum = gaps;
    let mut any_max_min = false;
    for i in 0..span {
        let t = &cols.tracks[c0 + i];
        let min_kind = if t.has_min != 0 { t.min_kind } else { t.kind };
        affected[i] = min_kind == TRACK_MAX_CONTENT;
        beyond[i] = affected[i];
        any_max_min = any_max_min || affected[i];
        sum += base[i];
    }
    if any_max_min && contribution.max > sum {
        distribute_extra(
            &mut base,
            &caps,
            &affected,
            &beyond,
            span,
            contribution.max - sum,
        );
        for i in 0..span {
            if affected[i] {
                col_min[c0 + i] = base[i];
            }
        }
    }
    for pass in 0..2 {
        let target = if pass == 0 {
            contribution.min
        } else {
            contribution.max
        };
        sum = gaps;
        for i in 0..span {
            let t = &cols.tracks[c0 + i];
            limit[i] = if max_is_intrinsic(t) {
                fmax(col_content[c0 + i], base[i])
            } else {
                fmax(fixed_px(t, avail), base[i])
            };
            caps[i] = f64::INFINITY;
            affected[i] = if pass == 0 {
                max_is_intrinsic(t)
            } else {
                t.kind == TRACK_AUTO || t.kind == TRACK_MAX_CONTENT
            };
            beyond[i] = affected[i];
            sum += limit[i];
        }
        if target <= sum {
            continue;
        }
        distribute_extra(&mut limit, &caps, &affected, &beyond, span, target - sum);
        for i in 0..span {
            if affected[i] {
                col_content[c0 + i] = limit[i];
            }
        }
    }
    true
}

pub(crate) struct RowTracks<'a> {
    pub template: Option<&'a GridTracks>,
    pub auto: Option<&'a GridTracks>,
    pub explicit: i32,
}

impl<'a> RowTracks<'a> {
    pub fn track(&self, r: i32) -> Option<&'a GridTrack> {
        if let Some(t) = self.template
            && r < t.n
        {
            return Some(&t.tracks[r as usize]);
        }
        let auto = self.auto.filter(|a| a.n > 0)?;
        Some(&auto.tracks[((r - self.explicit) % auto.n).max(0) as usize])
    }
}

pub(crate) fn expand_flexible_rows(row_height: &mut [f64], rows: &RowTracks<'_>, mut space: f64) {
    let n_rows = row_height.len();
    let mut fr_factor = vec![0.0f64; n_rows + 1];
    let mut any_fr = false;
    for (r, factor) in fr_factor.iter_mut().enumerate().take(n_rows) {
        if let Some(tk) = rows.track(r as i32)
            && tk.kind == TRACK_FR
            && tk.v > 0.0
        {
            *factor = tk.v;
            any_fr = true;
        }
    }
    if !any_fr {
        return;
    }
    for r in 0..n_rows {
        if fr_factor[r] <= 0.0 {
            space -= row_height[r];
        }
    }
    let mut inflexible = vec![false; n_rows + 1];
    for _ in 0..=n_rows {
        let mut sum_fr = 0.0;
        let mut leftover = space;
        for r in 0..n_rows {
            if fr_factor[r] <= 0.0 {
                continue;
            }
            if inflexible[r] {
                leftover -= row_height[r];
            } else {
                sum_fr += fr_factor[r];
            }
        }
        if sum_fr <= 0.0 {
            break;
        }
        let unit = if leftover > 0.0 {
            leftover / fmax(sum_fr, 1.0)
        } else {
            0.0
        };
        let mut changed = false;
        for r in 0..n_rows {
            if fr_factor[r] <= 0.0 || inflexible[r] {
                continue;
            }
            if row_height[r] > unit * fr_factor[r] + 0.01 {
                inflexible[r] = true;
                changed = true;
            }
        }
        if changed {
            continue;
        }
        for r in 0..n_rows {
            if fr_factor[r] > 0.0 && !inflexible[r] {
                row_height[r] = unit * fr_factor[r];
            }
        }
        break;
    }
}

pub(crate) fn extend_with_auto_tracks(
    tracks: &mut GridTracks,
    from: i32,
    to: i32,
    pattern: Option<&GridTracks>,
) {
    let pattern = pattern.filter(|p| p.n > 0 && p.subgrid == 0);
    let to = to.min(MAX as i32);
    for i in from..to {
        tracks.tracks[i as usize] = match pattern {
            Some(p) => p.tracks[((i - from) % p.n) as usize],
            None => auto_track(),
        };
    }
    if to > from {
        tracks.n = to;
    }
}
