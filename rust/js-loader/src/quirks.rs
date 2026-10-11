//! Southstar — the one site-specific script left from js.c, run after each classic script: it fills in tile matching defaults the freecivweb.com client leaves undefined.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use southstar_js_engine::{Scope, ScriptOptions};

const SITE_QUIRKS: &str = concat!(
    "(function(){",
    "try{",
    "var l=location&&location.hostname||'';",
    "if(l!=='freecivweb.com'&&l!=='www.freecivweb.com'&&l!=='fcw.movingborders.es')return;",
    "if(typeof tile_types_setup!=='object'||!tile_types_setup)return;",
    "if(typeof MATCH_NONE!=='number'||typeof MATCH_SAME!=='number'||",
    "typeof MATCH_PAIR!=='number'||typeof MATCH_FULL!=='number'||",
    "typeof CELL_WHOLE!=='number'||typeof CELL_CORNER!=='number')return;",
    "var r=typeof MATCH_RANDOM==='number'?MATCH_RANDOM:4,t=tile_types_setup;",
    "function s(k,m,c){var o=t[k];if(!o)return;if(o.match_style===void 0)o.match_style=m;if(o.sprite_type===void 0)o.sprite_type=c;}",
    "s('l0.lake',MATCH_PAIR,CELL_CORNER);",
    "s('l0.coast',MATCH_FULL,CELL_CORNER);",
    "s('l1.coast',MATCH_PAIR,CELL_CORNER);",
    "s('l0.floor',MATCH_FULL,CELL_CORNER);",
    "s('l1.floor',MATCH_PAIR,CELL_CORNER);",
    "s('l0.arctic',MATCH_NONE,CELL_WHOLE);",
    "s('l0.desert',MATCH_NONE,CELL_WHOLE);",
    "s('l1.desert',r,CELL_WHOLE);",
    "s('l0.forest',MATCH_NONE,CELL_WHOLE);",
    "s('l1.forest',MATCH_SAME,CELL_WHOLE);",
    "s('l0.grassland',MATCH_NONE,CELL_WHOLE);",
    "s('l1.grassland',r,CELL_WHOLE);",
    "s('l0.hills',MATCH_NONE,CELL_WHOLE);",
    "s('l1.hills',MATCH_SAME,CELL_WHOLE);",
    "s('l0.jungle',MATCH_NONE,CELL_WHOLE);",
    "s('l1.jungle',MATCH_SAME,CELL_WHOLE);",
    "s('l0.mountains',MATCH_NONE,CELL_WHOLE);",
    "s('l1.mountains',MATCH_SAME,CELL_WHOLE);",
    "s('l0.plains',MATCH_NONE,CELL_WHOLE);",
    "s('l1.plains',r,CELL_WHOLE);",
    "s('l0.swamp',MATCH_NONE,CELL_WHOLE);",
    "s('l1.swamp',r,CELL_WHOLE);",
    "s('l0.tundra',MATCH_NONE,CELL_WHOLE);",
    "s('l0.inaccessible',MATCH_NONE,CELL_WHOLE);",
    "}catch(e){}",
    "})()",
);

pub(crate) fn applies_to(url: &[u8]) -> bool {
    let has = |needle: &[u8]| url.windows(needle.len()).any(|window| window == needle);
    has(b"freecivweb.com") || has(b"fcw.movingborders.es")
}

pub(crate) fn apply(scope: &mut Scope<'_>) {
    let options = ScriptOptions {
        name: Some(b"site-quirks"),
        line: 1,
        ..ScriptOptions::default()
    };
    let _ = scope.eval_script_bytes(SITE_QUIRKS.as_bytes(), &options);
}
