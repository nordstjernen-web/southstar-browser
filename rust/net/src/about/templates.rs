//! Southstar — the about: page templates: the new-tab page, about:southstar and the settings page, with the placeholders filled per request.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use std::sync::OnceLock;

use southstar_about_style::base_css;
use southstar_config::DEFAULT_SEARCH_ENGINE;

use crate::about::l10n::localize;

pub const VERSION: &str = match option_env!("NS_VERSION") {
    Some(version) => version,
    None => env!("CARGO_PKG_VERSION"),
};

macro_rules! nav_icon {
    ($paths:expr) => {
        concat!(
            "<svg width=\"16\" height=\"16\" viewBox=\"0 0 16 16\" fill=\"none\" ",
            "stroke=\"currentColor\" stroke-width=\"1.5\" stroke-linecap=\"round\" ",
            "stroke-linejoin=\"round\">",
            $paths,
            "</svg>"
        )
    };
}

macro_rules! switch {
    ($id:expr, $label:expr, $desc:expr) => {
        concat!(
            "<label class=\"row\"><span class=\"txt\">",
            "<span class=\"lbl\">",
            $label,
            "</span>",
            "<span class=\"desc\">",
            $desc,
            "</span></span>",
            "<span class=\"sw\"><input type=\"checkbox\" id=\"",
            $id,
            "\">",
            "<span></span></span></label>"
        )
    };
}

const START_HEAD: &str = concat!(
    "<!doctype html><html lang=\"__ND_LANG__\"><head>",
    "<meta charset=\"utf-8\">",
    "<meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">",
    "<meta name=\"color-scheme\" content=\"light dark\">",
    "<title>{{New Tab}}</title>",
    "<style>\n",
);

const START_BODY: &str = concat!(
    "html,body{height:100%}\n",
    "body{background:radial-gradient(1100px 520px at 50% -140px,",
    "rgba(83,130,255,.22),transparent),",
    "radial-gradient(700px 380px at 85% 0,rgba(40,190,200,.12),transparent),",
    "var(--bg)}\n",
    ".ntp{max-width:640px;margin:0 auto;padding:13vh 24px 40px;",
    "display:flex;flex-direction:column;align-items:center}\n",
    ".vh{position:absolute;width:1px;height:1px;margin:-1px;padding:0;",
    "overflow:hidden;clip:rect(0 0 0 0);white-space:nowrap;border:0}\n",
    ".mark-img{display:block;width:48px;height:48px;border-radius:13px;",
    "margin-bottom:22px;box-shadow:0 6px 18px rgba(20,40,110,.25)}\n",
    ".search{display:flex;align-items:center;gap:12px;width:100%;",
    "height:56px;padding:0 6px 0 20px;border-radius:999px;",
    "background:var(--card);border:1px solid var(--line);",
    "box-shadow:var(--shadow)}\n",
    ".search:focus-within{border-color:var(--accent);",
    "box-shadow:0 0 0 4px var(--accent-soft),var(--shadow)}\n",
    ".search svg{flex:0 0 auto;color:var(--faint)}\n",
    ".search input{flex:1 1 auto;min-width:0;height:100%;border:0;",
    "background:transparent;color:var(--text);font:inherit;",
    "font-size:17px}\n",
    ".search input:focus-visible{outline:0}\n",
    ".search button{flex:0 0 auto;display:flex;align-items:center;",
    "justify-content:center;width:44px;height:44px;border:0;",
    "border-radius:50%;background:var(--accent);color:var(--accent-ink);",
    "cursor:pointer}\n",
    ".search button svg{color:var(--accent-ink)}\n",
    ".search button:hover{filter:brightness(1.08)}\n",
    ".links{display:flex;flex-wrap:wrap;align-items:center;justify-content:center;",
    "gap:4px 6px;margin-top:22px;font-size:14px}\n",
    ".links a{padding:4px 10px;border-radius:999px;color:var(--muted)}\n",
    ".links a:hover{color:var(--text);background:var(--field);",
    "text-decoration:none}\n",
    ".links span{color:var(--faint)}\n",
    ".foot{margin-top:48px;color:var(--faint);font-size:13px}\n",
    ".foot a{color:var(--muted)}\n",
    "@media (prefers-color-scheme:dark){body{background:",
    "radial-gradient(1100px 520px at 50% -140px,rgba(70,110,255,.28),",
    "transparent),radial-gradient(700px 380px at 85% 0,",
    "rgba(40,190,200,.10),transparent),var(--bg)}}\n",
    "@media (max-width:560px){.ntp{padding:9vh 16px 32px}",
    ".search{height:52px;padding-left:16px}.search input{font-size:16px}}\n",
    "</style></head>",
    "<body><main class=\"ntp\">",
    "<h1 class=\"vh\">{{New Tab}}</h1>",
    "__ND_LOGO_MARK__",
    "<form class=\"search\" id=\"dsearch\" role=\"search\">",
    "<svg aria-hidden=\"true\" width=\"20\" height=\"20\" viewBox=\"0 0 16 16\"",
    " fill=\"none\" stroke=\"currentColor\" stroke-width=\"1.6\"",
    " stroke-linecap=\"round\">",
    "<circle cx=\"7\" cy=\"7\" r=\"4.6\"/><path d=\"M10.4 10.4 14 14\"/></svg>",
    "<input id=\"sq\" name=\"q\" autocomplete=\"off\" autofocus dir=\"auto\"",
    " aria-label=\"{{Search with __ND_SEARCH_NAME__ or enter an address}}\"",
    " placeholder=\"{{Search or enter address}}\">",
    "<button type=\"submit\" aria-label=\"{{Search}}\">",
    "<svg aria-hidden=\"true\" width=\"18\" height=\"18\" viewBox=\"0 0 16 16\"",
    " fill=\"none\" stroke=\"currentColor\" stroke-width=\"1.8\"",
    " stroke-linecap=\"round\" stroke-linejoin=\"round\">",
    "<path d=\"M3 8h9.5M8.5 4l4 4-4 4\"/></svg>",
    "</button></form>",
    "<nav class=\"links\" aria-label=\"{{Browser pages}}\">",
    "<a href=\"about:history\">{{History}}</a><span aria-hidden=\"true\">\u{b7}</span>",
    "<a href=\"about:settings\">{{Settings}}</a><span aria-hidden=\"true\">\u{b7}</span>",
    "<a href=\"about:southstar\">{{About Southstar}}</a>",
    "</nav>",
    "<p class=\"foot\"><a href=\"about:southstar\">Southstar ",
    "__ND_VERSION__</a></p>",
    "</main>",
    "<script>\n",
    "var searchUrl='__ND_SEARCH_URL__';\n",
    "var dsearch=document.getElementById('dsearch');\n",
    "var sq=document.getElementById('sq');\n",
    "function addressFor(s){\n",
    " if(/^(about|file|data|view-source):/i.test(s)||s.indexOf('://')>=0) return s;\n",
    " if(!/\\s/.test(s)&&/^(localhost|[^\\s.\\/:]+(\\.[^\\s.\\/:]+)+)",
    "(:\\d+)?(\\/.*)?$/i.test(s)) return 'https://'+s;\n",
    " return null;}\n",
    "dsearch.addEventListener('submit',function(e){e.preventDefault();\n",
    " var s=sq.value.trim(); if(!s) return;\n",
    " var a=addressFor(s); if(a){window.location.href=a;return;}\n",
    " window.location.href=searchUrl.indexOf('%s')>=0\n",
    "  ? searchUrl.replace('%s',encodeURIComponent(s))\n",
    "  : searchUrl+encodeURIComponent(s);});\n",
    "try{sq.focus();}catch(e){}\n",
    "</script></body></html>",
);

const SOUTHSTAR_HEAD: &str = concat!(
    "<!doctype html><html lang=\"__ND_LANG__\"><head>",
    "<meta charset=\"utf-8\">",
    "<meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">",
    "<meta name=\"color-scheme\" content=\"light dark\">",
    "<title>{{About Southstar}}</title>",
    "<style>\n",
);

const SOUTHSTAR_BODY: &str = concat!(
    ".wrap{max-width:760px;margin:0 auto;padding:40px 24px 48px}\n",
    ".head{display:flex;align-items:center;gap:18px;margin:0 4px 22px}\n",
    ".mark-img{display:block;flex:0 0 auto;width:64px;height:64px;",
    "border-radius:17px;box-shadow:0 8px 24px rgba(20,40,110,.25)}\n",
    ".head h1{margin:0;font-size:30px;font-weight:700;letter-spacing:-.02em;",
    "line-height:1.15}\n",
    ".head p{margin:2px 0 0;color:var(--muted)}\n",
    ".head .built{font-size:13px;color:var(--faint)}\n",
    ".ver{display:inline-block;margin-left:10px;padding:3px 10px;",
    "border-radius:999px;background:var(--accent-soft);color:var(--accent);",
    "font-size:13px;font-weight:700;vertical-align:middle;letter-spacing:0}\n",
    ".card{padding:22px 24px;margin-bottom:16px}\n",
    ".card h2{margin:0 0 8px;font-size:17px}\n",
    ".card p{margin:0 0 10px;color:var(--muted);font-size:14px}\n",
    ".actions{display:flex;flex-wrap:wrap;gap:10px;margin-top:14px}\n",
    ".head-actions{margin:0 4px 22px}\n",
    "details.card{padding:0}\n",
    "details summary{display:flex;align-items:center;gap:10px;",
    "padding:18px 24px;cursor:pointer;list-style:none}\n",
    "details summary::-webkit-details-marker{display:none}\n",
    "details summary::before{content:\"\\25B8\";color:var(--faint)}\n",
    "details[open] summary::before{content:\"\\25BE\"}\n",
    "details summary h2{margin:0;font-size:17px}\n",
    ".diag{padding:0 24px 18px}\n",
    ".diag h3{margin:20px 0 6px;font-size:12px;font-weight:700;",
    "letter-spacing:.08em;text-transform:uppercase;color:var(--faint)}\n",
    ".diag h3:first-child{margin-top:0}\n",
    ".drow{display:flex;justify-content:space-between;gap:20px;",
    "padding:8px 0;border-bottom:1px solid var(--line);font-size:13.5px}\n",
    ".drow:last-child{border-bottom:0}\n",
    ".dk{color:var(--muted);flex:0 0 auto}\n",
    ".dv{min-width:0;color:var(--text);text-align:right;",
    "overflow-wrap:anywhere;font-family:var(--mono);font-size:12.5px}\n",
    ".dv.on,.dv.off{font-family:var(--font);font-weight:600;font-size:12px;",
    "padding:1px 10px;border-radius:999px}\n",
    ".dv.on{color:var(--ok);background:rgba(19,128,74,.1)}\n",
    ".dv.off{color:var(--faint);background:var(--field)}\n",
    ".foot{display:flex;flex-wrap:wrap;justify-content:space-between;",
    "gap:12px;margin-top:26px;color:var(--faint);font-size:13px}\n",
    ".foot a{color:var(--muted)}\n",
    "@media (max-width:560px){.wrap{padding:24px 16px 40px}",
    ".head{flex-direction:column;align-items:flex-start}",
    ".drow{flex-direction:column;gap:2px}",
    ".dv{text-align:left}",
    ".dv.on,.dv.off{align-self:flex-start}}\n",
    "</style></head>",
    "<body><main class=\"wrap\">",
    "<header class=\"head\">__ND_LOGO_MARK__<div>",
    "<h1>Southstar<span class=\"ver\">",
);

const SOUTHSTAR_TAIL: &str = concat!(
    "</span></h1>",
    "<p>{{A web browser written from scratch in Rust.}}</p>",
    "__ND_BUILT__</div></header>",
    "<div class=\"actions head-actions\">",
    "<button id=\"copy\" class=\"btn primary\" type=\"button\">",
    "{{Copy version info}}</button>",
    "<a class=\"btn\" href=\"https://github.com/nordstjernen-web/southstar-browser\">",
    "{{Source code}}</a></div>",
    "<section class=\"card\">",
    "<h2>{{Privacy}}</h2>",
    "<p>{{Southstar sends no telemetry. Its only connection to a third party ",
    "is Google Safe Browsing, in builds that include a key; you can turn it ",
    "off in Settings \u{2192} Privacy.}}</p>",
    "<div class=\"actions\">",
    "<a class=\"btn\" href=\"https://github.com/nordstjernen-web/southstar-browser/blob/main/docs/privacy-policy.md\">",
    "{{Privacy policy}}</a>",
    "<a class=\"btn\" href=\"about:settings#privacy\">{{Privacy settings}}</a>",
    "</div></section>",
    "<section class=\"card\">",
    "<h2>{{License}}</h2>",
    "<p>{{Free software under NSL-1.0 or GPL-3.0-or-later, with no warranty.}}</p>",
    "<div class=\"actions\">",
    "<a class=\"btn\" href=\"about:license\">Nordstjernen Source License</a>",
    "<a class=\"btn\" href=\"about:gpl\">GNU GPL v3</a>",
    "<a class=\"btn\" href=\"about:third-party\">{{Third-party notices}}</a>",
    "</div></section>",
    "<details class=\"card\"><summary><h2>{{Technical details}}</h2></summary>",
    "__ND_DIAG__</details>",
    "<footer class=\"foot\"><span>Southstar Browser \u{a9} 2026 ",
    "Andreas R\u{f8}sdal</span></footer>",
    "</main>",
    "<script>\n",
    "var copy=document.getElementById('copy');\n",
    "function versionText(){var lines=[];\n",
    " var nodes=document.querySelectorAll('#diag h3, #diag .drow');\n",
    " for(var i=0;i<nodes.length;i++){var n=nodes[i];\n",
    "  if(n.tagName==='H3'){if(lines.length)lines.push('');",
    "lines.push('['+n.textContent+']');continue;}\n",
    "  lines.push(n.children[0].textContent+': '+n.children[1].textContent);}\n",
    " return lines.join('\\n')+'\\n';}\n",
    "copy.addEventListener('click',function(){\n",
    " var label=copy.textContent;\n",
    " function done(t){copy.textContent=t;",
    "setTimeout(function(){copy.textContent=label;},1600);}\n",
    " try{navigator.clipboard.writeText(versionText()).then(",
    "function(){done('{{js:Copied}}');},function(){done('{{js:Could not copy}}');});}",
    "catch(e){done('{{js:Could not copy}}');}});\n",
    "</script></body></html>",
);

const SETTINGS_HEAD: &str = concat!(
    "<!doctype html><html lang=\"__ND_LANG__\"><head><meta charset=\"utf-8\">\n",
    "<meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\n",
    "<meta name=\"color-scheme\" content=\"light dark\">\n",
    "<title>{{Settings}}</title>\n",
    "<style>\n",
);

const SETTINGS_BODY: &str = concat!(
    ".layout{display:flex;gap:32px;max-width:980px;margin:0 auto;",
    "padding:36px 28px 60px}\n",
    ".side{flex:0 0 200px;position:sticky;top:36px;align-self:flex-start}\n",
    ".side h1{margin:0 0 18px 12px;font-size:24px;letter-spacing:-.02em}\n",
    ".side a{display:flex;align-items:center;gap:10px;height:38px;",
    "padding:0 12px;border-radius:999px;color:var(--muted);font-size:14px;",
    "font-weight:600}\n",
    ".side a:hover{background:var(--field);color:var(--text);",
    "text-decoration:none}\n",
    ".side svg{color:var(--accent)}\n",
    "main{flex:1 1 auto;min-width:0}\n",
    "section{margin:0 0 26px}\n",
    "section h2{margin:0 0 10px 6px;font-size:15px;font-weight:700}\n",
    ".card{padding:6px 22px}\n",
    ".row{display:flex;align-items:center;justify-content:space-between;",
    "gap:20px;padding:14px 0;cursor:pointer}\n",
    ".card > * + *{border-top:1px solid var(--line)}\n",
    ".txt{display:flex;flex-direction:column;gap:2px;min-width:0}\n",
    ".lbl{font-size:14.5px;font-weight:600}\n",
    ".desc{font-size:13px;color:var(--muted)}\n",
    ".field{display:flex;flex-direction:column;gap:8px;padding:14px 0}\n",
    "input[type=text],select{width:100%;padding:9px 12px;",
    "border-radius:12px;border:1px solid var(--line);background:var(--field);",
    "color:var(--text);font:inherit;font-size:14px;outline:none}\n",
    "input[type=text]:focus,select:focus{border-color:var(--accent);",
    "box-shadow:0 0 0 3px var(--accent-soft)}\n",
    ".sw{position:relative;flex:0 0 auto;width:42px;height:24px}\n",
    ".sw input{position:absolute;opacity:0;width:0;height:0;margin:0}\n",
    ".sw span{position:absolute;top:0;left:0;right:0;bottom:0;",
    "border-radius:999px;background:rgba(120,128,145,.35);",
    "transition:background .18s}\n",
    ".sw span::after{content:\"\";position:absolute;top:3px;left:3px;",
    "width:18px;height:18px;border-radius:50%;background:#fff;",
    "box-shadow:0 1px 3px rgba(0,0,0,.3);transition:left .18s}\n",
    ".sw input:checked + span{background:var(--accent)}\n",
    ".sw input:checked + span::after{left:21px}\n",
    ".sw input:focus-visible + span{box-shadow:0 0 0 3px var(--accent-soft)}\n",
    ".danger-row{display:flex;align-items:center;justify-content:space-between;",
    "gap:20px;padding:16px 0}\n",
    "#custom_wrap[hidden]{display:none}\n",
    ".save-row{display:flex;justify-content:flex-end;padding:14px 0}\n",
    ".toast{position:fixed;left:50%;bottom:28px;transform:translateX(-50%);",
    "padding:10px 18px;border-radius:999px;background:var(--text);",
    "color:var(--bg);font-size:13.5px;font-weight:600;",
    "box-shadow:0 8px 24px rgba(0,0,0,.2);opacity:0;transition:opacity .2s}\n",
    ".toast.show{opacity:1}\n",
    "@media (max-width:760px){.layout{flex-direction:column;gap:8px;",
    "padding:24px 18px 48px}.side{position:static}",
    ".side a{display:none}}\n",
    "</style></head><body>\n",
    "<div class=\"layout\">\n",
    "<nav class=\"side\"><h1>{{Settings}}</h1>",
    "<a href=\"#general\">",
    nav_icon!("<path d=\"M2.25 7.5 8 2.5l5.75 5\"/><path d=\"M3.75 6.5v7h8.5v-7\"/>"),
    "{{General}}</a>",
    "<a href=\"#privacy\">",
    nav_icon!(
        "<path d=\"M8 1.75 13.25 3.75v4c0 3.25-2.25 5.5-5.25 6.5-3-1-5.25-3.25-5.25-6.5v-4z\"/>"
    ),
    "{{Privacy}}</a>",
    "<a href=\"#content\">",
    nav_icon!(
        "<rect x=\"2.25\" y=\"2.75\" width=\"11.5\" height=\"10.5\" rx=\"2\"/><path d=\"M2.25 6h11.5\"/>"
    ),
    "{{Content}}</a>",
    "<a href=\"#data\">",
    nav_icon!("<path d=\"M3.25 4.5h9.5M6.5 4.5V3h3v1.5M4.5 4.5l.6 8.75h5.8l.6-8.75\"/>"),
    "{{Browsing data}}</a></nav>\n",
    "<main>\n",
    "<section id=\"general\"><h2>{{General}}</h2><div class=\"card\">",
    "<label class=\"field\"><span class=\"lbl\">{{Home page}}</span>",
    "<input id=\"home_url\" type=\"text\" spellcheck=\"false\"></label>",
    "<label class=\"field\"><span class=\"lbl\">{{Search engine}}</span>",
    "<select id=\"search_pick\"></select></label>",
    "<label class=\"field\" id=\"custom_wrap\"><span class=\"lbl\">",
    "{{Custom search URL}}</span>",
    "<input id=\"search_engine\" type=\"text\" spellcheck=\"false\" ",
    "placeholder=\"https://example.com/?q=%s\"></label>",
    "<div class=\"save-row\">",
    "<button id=\"save\" class=\"btn primary\" type=\"button\">{{Save}}</button>",
    "</div></div></section>\n",
    "<section id=\"privacy\"><h2>{{Privacy}}</h2><div class=\"card\">",
    "<label class=\"field\"><span class=\"lbl\">{{Cookies}}</span>",
    "<select id=\"cookie_policy\">",
    "<option value=\"0\">{{Accept all cookies}}</option>",
    "<option value=\"1\">{{Block third-party cookies}}</option>",
    "<option value=\"2\">{{Block all cookies}}</option></select></label>\n",
    switch!(
        "do_not_track",
        "{{Send \u{201c}Do Not Track\u{201d}}}",
        "{{Ask every site not to track you.}}"
    ),
    switch!(
        "global_privacy_control",
        "{{Send Global Privacy Control}}",
        "{{Tell sites not to sell or share your data.}}"
    ),
    switch!(
        "safe_browsing",
        "{{Block dangerous sites}}",
        "{{Warn before opening known phishing and malware sites, using Google Safe Browsing and a local list.}}"
    ),
    switch!(
        "strip_tracking_params",
        "{{Strip tracking parameters}}",
        "{{Remove utm_ and similar tags from links you open.}}"
    ),
    switch!(
        "https_first",
        "{{HTTPS first}}",
        "{{Try a secure connection before plain HTTP on every site.}}"
    ),
    switch!(
        "remote_control",
        "{{Allow remote control}}",
        "{{Let a local agent such as Claude Code drive this browser over MCP.}}"
    ),
    "</div></section>\n",
    "<section id=\"content\"><h2>{{Content}}</h2><div class=\"card\">\n",
    switch!(
        "images_enabled",
        "{{Load images}}",
        "{{Show pictures on web pages.}}"
    ),
    switch!(
        "javascript_enabled",
        "{{JavaScript}}",
        "{{Run scripts that pages need for interactive features.}}"
    ),
    switch!(
        "webgl_enabled",
        "{{WebGL}}",
        "{{Allow 3D graphics in the canvas element.}}"
    ),
    switch!(
        "local_storage_enabled",
        "{{Local storage}}",
        "{{Let sites keep data on this device.}}"
    ),
    switch!(
        "cache_enabled",
        "{{Cache}}",
        "{{Keep copies of pages and files to load them faster.}}"
    ),
    "</div></section>\n",
    "<section id=\"data\"><h2>{{Browsing data}}</h2><div class=\"card\">",
    "<div class=\"danger-row\"><span class=\"txt\"><span class=\"lbl\">",
    "{{Clear all browsing data}}</span><span class=\"desc\">{{History, cookies, ",
    "cache and site storage on this device.}}</span></span>",
    "<button id=\"clear\" class=\"btn danger\">{{Clear data}}</button></div>",
    "</div></section>\n",
    "</main></div>\n",
    "<div class=\"toast\" id=\"status\" role=\"status\"></div>\n",
    "<script>\n",
    "function $(i){return document.getElementById(i);}\n",
    "function enc(o){var p=[];for(var k in o)p.push(encodeURIComponent(k)+'='+",
    "encodeURIComponent(o[k]==null?'':o[k]));return p.join('&');}\n",
    "var engines=[{n:'DuckDuckGo',u:'",
);

const SETTINGS_TAIL: &str = concat!(
    "'},",
    "{n:'Startpage',u:'https://www.startpage.com/sp/search?query=%s'},",
    "{n:'Google',u:'https://www.google.com/search?q=%s'},",
    "{n:'Bing',u:'https://www.bing.com/search?q=%s'},",
    "{n:'Brave',u:'https://search.brave.com/search?q=%s'},",
    "{n:'Wikipedia',u:'https://en.wikipedia.org/w/index.php?search=%s'}];\n",
    "var toggles=['do_not_track','global_privacy_control','safe_browsing',",
    "'strip_tracking_params','https_first','images_enabled',",
    "'javascript_enabled','webgl_enabled','local_storage_enabled',",
    "'cache_enabled','remote_control'];\n",
    "var loaded=false,toastTimer=0;\n",
    "function toast(t){var s=$('status');s.textContent=t;s.className='toast show';",
    "clearTimeout(toastTimer);toastTimer=setTimeout(function(){",
    "s.className='toast';},1800);}\n",
    "function buildPick(cur){var sel=$('search_pick');sel.textContent='';",
    "var matched=false;engines.forEach(function(e){var o=document.createElement(",
    "'option');o.value=e.u;o.textContent=e.n;if(e.u===cur){o.selected=true;",
    "matched=true;}sel.appendChild(o);});",
    "var oc=document.createElement('option');oc.value='custom';",
    "oc.textContent='{{js:Custom\u{2026}}}';if(!matched)oc.selected=true;",
    "sel.appendChild(oc);",
    "$('custom_wrap').hidden=matched;}\n",
    "function load(){fetch('about:settings-data').then(function(r){",
    "return r.json();}).then(function(c){",
    "$('home_url').value=c.home_url||'';",
    "$('search_engine').value=c.search_engine||'';buildPick(c.search_engine||'');",
    "$('cookie_policy').value=''+(c.cookie_policy||0);",
    "toggles.forEach(function(k){$(k).checked=!!c[k];});loaded=true;});}\n",
    "function bv(id){return $(id).checked?'1':'0';}\n",
    "function save(o){if(!loaded)return;",
    "fetch('about:settings-save',{method:'POST',headers:{'Content-Type':",
    "'application/x-www-form-urlencoded'},body:enc(o)}).then(function(){",
    "toast('{{js:Saved. Applies to newly opened pages.}}');});}\n",
    "function saveField(k){var o={};o[k]=$(k).value;save(o);}\n",
    "function saveGeneral(){var se=$('search_pick').value;",
    "if(se!=='custom')$('search_engine').value=se;",
    "save({home_url:$('home_url').value,search_engine:$('search_engine').value});}\n",
    "$('search_pick').onchange=function(){",
    "var v=this.value;$('custom_wrap').hidden=v!=='custom';",
    "if(v!=='custom'){$('search_engine').value=v;saveField('search_engine');}};\n",
    "['home_url','search_engine','cookie_policy'].forEach(function(k){",
    "$(k).onchange=function(){saveField(k);};});\n",
    "toggles.forEach(function(k){$(k).onchange=function(){",
    "var o={};o[k]=bv(k);save(o);};});\n",
    "$('save').onclick=saveGeneral;\n",
    "$('clear').onclick=function(){var b=$('clear');b.disabled=true;",
    "fetch('about:settings-clear',{method:'POST'}).then(function(){",
    "b.textContent='{{js:Cleared}}';toast('{{js:Browsing data cleared}}');});};\n",
    "load();\n",
    "</script></body></html>",
);

fn joined(cell: &'static OnceLock<String>, parts: &[&str]) -> &'static str {
    cell.get_or_init(|| localize(&parts.concat()))
}

pub fn start() -> &'static str {
    static PAGE: OnceLock<String> = OnceLock::new();
    joined(&PAGE, &[START_HEAD, base_css(), START_BODY])
}

pub fn southstar() -> &'static str {
    static PAGE: OnceLock<String> = OnceLock::new();
    joined(
        &PAGE,
        &[
            SOUTHSTAR_HEAD,
            base_css(),
            SOUTHSTAR_BODY,
            VERSION,
            SOUTHSTAR_TAIL,
        ],
    )
}

pub fn settings() -> &'static str {
    static PAGE: OnceLock<String> = OnceLock::new();
    joined(
        &PAGE,
        &[
            SETTINGS_HEAD,
            base_css(),
            SETTINGS_BODY,
            DEFAULT_SEARCH_ENGINE,
            SETTINGS_TAIL,
        ],
    )
}
