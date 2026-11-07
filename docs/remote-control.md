# Remote control over MCP

A local AI agent such as [Claude Code](https://claude.com/claude-code)
can drive Southstar, much as the "Claude in Chrome" extension drives
Chrome. The browser ships its own
[Model Context Protocol](https://modelcontextprotocol.io) server.
`southstar --mcp` speaks MCP over stdio and forwards each tool call to
your open browser window. It goes through a control channel that listens
only on the loopback interface, and only while you allow it.

Nothing listens by default. Remote control is off until you turn it on.

## Setup

1. Register the MCP server with your agent. For Claude Code:

   ```sh
   # Linux / macOS (installed)
   claude mcp add southstar -- /usr/bin/southstar --mcp

   # A cargo xtask build tree
   claude mcp add southstar -- /path/to/southstar-browser/target/release/southstar --mcp

   # Windows (MSYS2 build tree)
   claude mcp add southstar -- C:\dev\southstar-browser\target\release\southstar.exe --mcp
   ```

   Use the path of your own `southstar` binary. Add `--scope user` to
   make it available in every project.

2. Allow remote control in the browser, in either of two ways:

   - **Settings → Privacy → Allow remote control** (the `remote_control`
     config key). It takes effect within two seconds in a running
     browser. Turning it off closes the channel.
   - Start the browser with `--remote-control` to allow it for that
     session only.

3. Ask the agent to use the browser ("open example.com in Southstar and
   click the first link").

The agent's first tool call may find no browser running with remote
control. `southstar --mcp` then starts one with `--remote-control` and
waits for its control channel. A browser that is already open with
remote control off is left alone, and the agent gets a second window.

While a client is connected, the toolbar shows an orange **Controlled
by …** badge that names the client (for example "Controlled by Claude
Code"). Click it to stop remote control for the rest of the session. The
channel closes, and the MCP server reports that the user stopped it
instead of starting another browser. To allow it again, turn the
Settings switch off and on, or restart the browser.

## Tools

Tools take an optional `tab_id` (from `tabs_list`). Without one, they act
on the active tab.

| Tool | What it does |
| --- | --- |
| `tabs_list` | Open tabs with id, window, URL, title and active/private/loading flags. |
| `tabs_create` | Open a foreground tab at `url` (default `about:blank`) and wait for it to load. |
| `tabs_close` | Close a tab. The browser refuses to close the last tab of the last window. |
| `tab_select` | Bring a tab to the front and present its window. |
| `navigate` | Load `url` and wait for the load to finish. `"back"` and `"forward"` walk the tab's history. Non-URL text is searched. |
| `reload` | Reload the page and wait for it. |
| `read_page` | An accessibility-tree-like outline: role, accessible name, state, viewport bounding box and a ref such as `ref_12`. `filter: "interactive"` lists only controls. `max_chars` caps the output (default 40000). |
| `find` | Elements that match a CSS selector, or whose text, accessible name, placeholder or value contains the query. Text inside a `<label>` resolves to its control. |
| `click` | A real mouse click on a `ref` (scrolled into view first) or at `x`/`y` (or `coordinate: [x, y]`) in viewport pixels, with optional `modifiers`. Ctrl or Meta opens a link in a new tab. Waits for any navigation to settle. |
| `double_click` | Two clicks with word selection, then a `dblclick` event. |
| `hover` | Moves the pointer over a `ref` or point, so hover styles and mouse events fire. |
| `type` | Inserts `text` into the focused element, or clicks `ref` first. `\n` presses Enter. |
| `key` | Key presses and shortcuts, space separated: `Enter`, `Tab`, `shift+Tab`, `ctrl+a`, `ArrowDown`, `PageDown`, `Escape`, `F5`, single characters. `repeat` repeats the sequence. |
| `scroll` | Mouse-wheel scroll `up`/`down`/`left`/`right` by `amount` ticks of 100 px, at a ref, a point or the viewport centre. A `ref` without a direction scrolls it into view. |
| `form_input` | Sets an input, textarea, select (by value or label), checkbox/radio (`true`/`false`) or contenteditable, and fires `input` and `change`. |
| `screenshot` | A PNG of the tab's viewport, returned as MCP image content. One image pixel is one CSS pixel, so screenshot coordinates are what `click`, `hover` and `scroll` take. |
| `javascript` | Evaluates `code` in the page's global scope and returns the last expression's value (objects as JSON, elements as HTML). A returned Promise is awaited for up to 30 s. |
| `get_page_text` | Title, URL and the body's rendered text. |
| `console` | The tab's console messages since the last call: page and worker `console.*` output, uncaught errors, unhandled rejections and resource errors. |
| `wait` | Sleeps for `ms` milliseconds (at most 30000). |

Refs are assigned per document and stay stable while it lives. After a
navigation, call `read_page` or `find` again.

## How it works

- **`rust/shell/src/ffi/remotectl.rs`** runs inside the GUI process.
  When remote control is allowed, it binds a `std::net::TcpListener` to
  `127.0.0.1` on a random port. It then writes `remote-control.json`
  (port, a 256-bit token from the OS random source, pid, version) to the
  user state directory: `$XDG_STATE_HOME/southstar/` or
  `~/.local/state/southstar/` on Linux and macOS, and
  `%LOCALAPPDATA%\southstar\` on Windows. The file has mode `0600`. The
  protocol is newline-delimited JSON, and every request carries the
  token. A thread per connection reads requests and hands each one to
  the GTK main loop, which runs it and sends the reply back.
- **`rust/shell/src/ffi/procview_remote.rs`** drives pages through the
  same tab-view input path the mouse and keyboard use (the view's click,
  release, select, hover, key, IME-text and wheel requests). Clicks
  therefore hit-test in the renderer, focus moves, and links navigate
  exactly as they do for the user. Script evaluation and console reads
  ride the view's worker requests `REQ_EVAL` and `REQ_CONSOLE`.
  Screenshots render the tab's drawing area through GSK at CSS-pixel
  size. Tabs are listed, opened, selected, loaded and closed through
  `rust/shell/src/ffi/window.rs`.
- **`rust/mcp`** (`southstar-mcp`) is the `--mcp` stdio server:
  JSON-RPC 2.0, one message per line, MCP protocol `2025-06-18`. It also
  answers clients that ask for `2025-03-26` or `2024-11-05`. It
  implements `initialize`, `notifications/initialized`, `ping`,
  `tools/list` and `tools/call`, and keeps one connection to the browser
  open for the session. It also holds the small JSON parser and writer
  that both ends use (`rust/mcp/src/json.rs`).
- **`rust/mcp/js/remote-page.js`** is the page-side helper that
  `read_page`, `find`, `form_input`, `get_page_text`, `double_click` and
  `javascript` evaluate through the renderer's console-eval path
  (origin `devtools-console`). It stores itself under
  `Symbol.for('southstar.remote')`.

## Security model

- **Off by default.** No socket exists unless the Settings switch is on
  or the browser was started with `--remote-control`. Turning the switch
  off closes the listener and every connection, and deletes the control
  file.
- **Loopback only.** The listener binds `127.0.0.1`, never a public
  interface.
- **A token per session.** The browser mints a fresh random token each
  time the channel opens. The token is stored only in the control file,
  which only your user account can read. A connection that does not
  present the token within 10 seconds is dropped, and a wrong token
  closes the connection. At most four connections are accepted at a
  time.
- **Out of the renderers' reach.** On Linux the Landlock rules give only
  the UI process access to the state directory. On macOS the seatbelt
  rules do the same. A compromised renderer therefore can neither read
  the token nor plant a forged file. The `--mcp` client also refuses a
  control file that is not a regular file owned by your user with no
  group or other permission bits. Renderers may not `listen` or
  `accept` sockets (seccomp).
- **Visible.** The toolbar badge shows while any authenticated client is
  connected, and clicking it ends remote control.
- **What a connected client can do** is what you can do at the keyboard:
  open, read and close tabs, type, click, run JavaScript in pages, and
  read page content, including pages you are logged in to. Only allow
  remote control for agents you trust, and stop it when you are done.
  Any program running as your user can read the control file, just as it
  could read your cookies.
- **The page's CSP.** The `javascript` tool's code may compile strings
  (the helper uses an indirect `eval`) even on a page whose
  Content-Security-Policy lacks `'unsafe-eval'`. This holds only while
  every script on the stack belongs to that devtools-console evaluation.
  Any page function on the stack keeps the page's policy in force, so
  pages cannot use the helper (which they can reach through
  `Symbol.for`) to evaluate strings.
- **Web content cannot change the setting.** `about:settings` answers
  only requests from the browser chrome.

## Limitations

- `screenshot` captures the viewport of a tab that is visible in its
  window. A minimised window cannot be captured.
- `type` inserts text the way an input method does: `input` events fire,
  but there is no `keydown` per character. Use `key` for key events.
- `double_click` dispatches its `dblclick` from script after two real
  clicks, so `event.isTrusted` is false for it. The renderer does not
  yet receive mouse buttons and click counts.
- On Windows the control file sits in `%LOCALAPPDATA%\southstar\`, next
  to the profile, and the renderer sandbox has no file-system rules to
  keep it from reading the file.
- File uploads, download dialogs, native popups and permission prompts
  are not driven.
