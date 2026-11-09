# Southstar for Java

**A web browser for the JVM** — and the library it is built on.
`org.southstar.app.Browser` is a standalone desktop browser written in plain
Java (Swing chrome) that drives Southstar's Rust engine in the sandboxed
`southstar-renderer` process; the same jar doubles as an embedding library
for fetching, laying out, scripting and rendering web pages from your own Java
code.

No engine code and no native library is loaded into the JVM. Every page runs
in a `southstar-renderer` process of its own, behind the renderer's sandbox
(seccomp and Landlock on Linux, Seatbelt on macOS, process mitigation
policies on Windows), and the JVM talks to it over the same stdio protocol the
GTK shell uses. A page that crashes or hangs costs that page and nothing else,
and the jar itself is pure Java: it runs wherever a JDK 21 and a
`southstar-renderer` for the platform are found.

**Requires JDK 21.**

## Build and run

```sh
# 1. Build the renderer (and the rest of Southstar) from the repository root:
cargo xtask build

# 2. Build the jar, with Gradle or Maven — they produce the same artifacts:
cd java && gradle build      # -> build/libs/southstar-java-<version>.jar
cd java && mvn package       # -> target/southstar-java-<version>.jar

# 3. Run the browser:
java -jar java/build/libs/southstar-java-<version>.jar https://example.com
# or from java/: gradle run --args="https://example.com"
```

The jar finds its renderer in this order: the `southstar.renderer` system
property, the `SOUTHSTAR_RENDERER` environment variable, a
`southstar-renderer` beside the jar, the `target/release/` and then
`target/debug/` of a source checkout up to six directories above the jar —
so a jar built in `java/` picks up the `cargo xtask build` output with no
configuration — and finally the directories on `PATH`. The working directory
is never searched, so a renderer planted next to a document you open cannot be
picked up. Page audio plays when a `southstar-audio` helper sits beside the
renderer.

The renderer is checked with the engine protocol handshake (`/hello`) when it
starts, and every request to it has a deadline (two minutes, or the
`southstar.timeout.ms` system property, or `Page.setTimeout`). A renderer that
does not answer in time is stopped and the call fails with
`RendererTimeoutException`; one that has crashed fails the next call at once
with its exit code. Exit code 70 means `NS_REQUIRE_SANDBOX` was set and the
renderer could not enter its sandbox.

## The browser

A Swing window with back, forward, reload, home, an address bar with a
transport-security badge, and a status bar. `Ctrl+N` opens another window
with a renderer of its own, and `Ctrl+Shift+N` a private one whose renderer
keeps no cookies, cache or history. The home page is `about:start`.

Beyond navigation it covers hover link previews and pointer shapes, zoom that
reflows the page, sharp rendering on HiDPI screens, find-in-page, drag text
selection with cut, copy and paste, a right-click menu the page can override,
camera permission prompts, downloads, dropping files onto the page, page
audio through the `southstar-audio` helper, page-requested full screen,
scrolling of overflow scrollers and the scrollbars a page paints itself,
back/forward-cache history traversal, saving the page as PDF or PNG, and a
JavaScript console with page-source, layout, network and performance dumps.

Media the engine does not play inline is not handed to an external player:
the browser copies its address to the clipboard and says so in the status
bar, as the GTK shell leaves it to the user.

### Keyboard & mouse

| Action | Keys |
| --- | --- |
| Back / Forward | `Alt+Left` / `Alt+Right`, or the mouse back/forward buttons |
| Reload | `Ctrl+R` or `F5` |
| Home | `Alt+Home` |
| Focus the URL bar | `Ctrl+L` or `Alt+D` |
| New window | `Ctrl+N` or `Ctrl+T` |
| New private window | `Ctrl+Shift+N` |
| Close the window / quit | `Ctrl+W` / `Ctrl+Q` |
| Find in page | `Ctrl+F`, then `Ctrl+G` / `Ctrl+Shift+G` |
| Zoom | `Ctrl` + `+` / `-` / `0`, or `Ctrl`+wheel |
| Cut / Copy / Paste / Select all | `Ctrl+X` / `Ctrl+C` / `Ctrl+V` / `Ctrl+A` (`Cmd` on macOS) |
| Save page as PDF | `Ctrl+P` |
| JavaScript console | `F12` or `Ctrl+Shift+J` |
| Scroll | `↑` / `↓` / `PageUp` / `PageDown` / `Space`, mouse wheel (`Shift` for sideways), or the scrollbar |
| Open a link in a new window | Middle click |
| Leave page full screen | `Esc` |

**Text input.** Click an input field, textarea or other editable element to
focus it, then type — characters, `Backspace`/`Delete`, caret movement
(`←`/`→`/`Home`/`End`), `Enter`, and `Tab` between fields are all forwarded to
the page. Key events go to whatever the page has focused, so the arrow keys
edit a focused field rather than scrolling. Paste goes to the focused field
too, and text a page copies lands on the system clipboard.

## Use as a library

### The browser API (`org.southstar.browser`)

`Engine`, `BrowserContext` and `Page` are the API to build on.

```java
import org.southstar.browser.BrowserContext;
import org.southstar.browser.Engine;
import org.southstar.browser.Page;
import org.southstar.browser.PdfOptions;
import org.southstar.browser.Response;

try (Engine engine = Engine.launch();
     BrowserContext context = engine.newContext();   // a profile of its own, deleted on close
     Page page = context.newPage(1280, 800)) {

    Response response = page.navigate("https://example.com");
    response.ok();                                   // false when no document opened
    page.title();                                    // also url(), security(), serverAddress()
    page.text();                                     // the text as laid out
    page.links();                                    // absolute link URLs
    page.evaluate("document.querySelectorAll('a').length");

    byte[] png = page.screenshot();                  // the whole page, encoded by the renderer
    byte[] hi  = page.screenshot(Page.Screenshot.region(0, 0, 640, 400).withScale(2));
    byte[] pdf = page.pdf(PdfOptions.a4().margins(15));

    page.click(120, 240);                            // real pointer and click events
    page.type("hello");                              // into the focused field
    page.press("Enter");
}
```

- **Any thread.** A page serves its requests one at a time, in the order they
  arrive; nothing is tied to the thread that created it.
- **Deadlines.** Every request has one (`Engine.Options.withTimeout`,
  `Page.setTimeout`; two minutes by default). A renderer that does not answer
  is stopped and the call fails with `RendererTimeoutException`. Navigating
  the page again starts a fresh renderer.
- **Profiles.** `engine.newContext()` gives a context whose pages share
  cookies and storage with each other and with nothing else, in a directory
  that is deleted when the context closes. `ContextOptions.persistent(dir)`
  keeps a profile between runs, `ContextOptions.inMemory()` writes nothing to
  disk, and `engine.userContext()` is this user's own Southstar profile.
- **HTML from a string.** `page.setContent(html, baseUrl)` shows markup you
  already hold; the base URL resolves its relative links and decides its
  origin. `HtmlRenderer` wraps the common case in one call:

  ```java
  try (HtmlRenderer renderer = HtmlRenderer.create()) {   // loads nothing from outside the document
      byte[] pdf = renderer.pdf("<h1>Invoice 42</h1>");
      byte[] png = renderer.png("<h1>Invoice 42</h1>", 800);
  }
  ```
- **What a page may load.** A `ResourcePolicy` covers every request a page
  causes, redirects, `fetch`, WebSocket and EventSource included:
  `ResourcePolicy.none()` (no network, no files), `publicWeb()` (no local
  files and no private or loopback address, also when a public name resolves
  to one), or `allowAll()` narrowed with `allowHosts("example.com",
  "*.example.com")`, `denyHosts(...)`, `files(false)` and
  `privateNetwork(false)`. Set it for a context with
  `ContextOptions.withResourcePolicy` or for one page with
  `page.setResourcePolicy`. Use `publicWeb()` or stricter for HTML and URLs
  you do not trust when the JVM can reach internal services.
- **How pages present themselves.** `ContextOptions` also takes
  `withUserAgent`, `withAcceptLanguage`, `withProxy` / `withProxyBypass`,
  `withJavaScript(false)`, `withImages(false)` and `withColorScheme`.
- **PDF.** `pdf()` lays the page out again for the sheet with its print
  styles and `@page` rule and cuts it into pages, with real text. `PdfOptions`
  sets the sheet, landscape, margins, scale, page ranges, the document's
  title, author and date, and header and footer texts with `{page}`,
  `{pages}`, `{title}`, `{url}` and `{date}`; see `docs/printing.md`.
- **No `java.desktop`.** `screenshot()` returns PNG bytes and `render()` raw
  premultiplied BGRA rows. `org.southstar.swing.FrameImages` turns those into
  a `BufferedImage` for a Swing or JavaFX host.
- **A host's side of the page.** `render()` and `tick()` report what the page
  asked for (`PageEvents`): a navigation, a download (`startDownload`,
  `pollDownload`, `saveDownload`), camera requests (`resolveCapability`),
  audio commands for `AudioHelper`, clipboard writes, WebGL use and window
  actions (full screen, print).

`examples/PageExample.java` is a runnable version of the snippet above:

```sh
javac -cp java/build/libs/southstar-java-<version>.jar -d out java/examples/PageExample.java
java -cp java/build/libs/southstar-java-<version>.jar:out PageExample https://example.com
```

### Agents and the MCP server (`org.southstar.agent`)

`AgentBrowser` is the browser API with a window's half added, for a program
or a model that drives pages rather than renders them. Its tabs keep their
scroll position and history, follow the navigations a click, a form or a
script asks for, save downloads, and wait for the page to settle after every
action. A page is described as an outline whose elements carry references:

```java
try (AgentBrowser browser = AgentBrowser.launch()) {
    Tab tab = browser.newTab("https://example.com");
    String outline = tab.snapshot(false, 40_000);
    tab.click("ref_3");                       // scrolls it into view, clicks, follows the link
    tab.type("ref_7", "hello\n");             // click a field, type, press Enter
    tab.setValue("ref_9", "Blue");            // a select by label, a checkbox by true/false
    tab.press("ctrl+a", 1);
    String found = tab.find("Sign in", 10);   // by text, name, placeholder or CSS selector
    byte[] png = tab.screenshot();            // the viewport; click(x, y) takes its pixels
    String value = tab.evaluate("document.title");
}
```

Every action returns a short account of what happened, ending in the tab's
URL and title, and a failed one throws `AgentException` with a message
written for the model that asked (`unknown or stale ref ref_4 (call
read_page or find again)`).

`BrowserTools` offers the same actions as tools: a name, a description and a
JSON Schema each, and `call(name, argumentsJson)` to run one. The names and
arguments are those of the desktop browser's MCP server (`southstar --mcp`):
`navigate`, `read_page`, `find`, `click`, `double_click`, `hover`, `type`,
`key`, `scroll`, `form_input`, `screenshot`, `javascript`, `get_page_text`,
`wait`, `save_pdf`, `reload` and the `tabs_*` tools.

`McpServer` serves those tools over the Model Context Protocol on stdio, with
no dependency beyond the jar:

```sh
java -cp southstar-java.jar org.southstar.agent.McpServer \
     --profile ~/.cache/agent-profile --downloads ~/Downloads --policy public-web
```

```json
{ "mcpServers": { "southstar": {
    "command": "java",
    "args": ["-cp", "/path/to/southstar-java.jar", "org.southstar.agent.McpServer"] } } }
```

`--policy public-web` keeps the browser off local files and private
addresses, `--allow-host` / `--deny-host` narrow it further, and `--help`
lists the rest. Unlike `southstar --mcp`, which drives the window on the
user's screen, this one is headless and needs no display.

### A page in a Swing application (`org.southstar.swing.BrowserView`)

`BrowserView` is a `JPanel` that shows a page and lets the user work it. It is
the part of the Java browser that is not window chrome, and the browser itself
is built on it:

```java
Engine engine = Engine.launch();
BrowserView view = new BrowserView(engine.newContext().newPage());
frame.add(view, BorderLayout.CENTER);
view.navigate("https://example.com");

view.addListener(new BrowserView.Listener() {
    @Override public void titleChanged(String title) { frame.setTitle(title); }
    @Override public void urlChanged(String url)     { address.setText(url); }
    @Override public void statusChanged(String text) { status.setText(text); }
});
```

The view paints the page (sharp on HiDPI screens), scrolls it with its own
scrollbars, the wheel and the keyboard, zooms, selects text, cuts, copies and
pastes with the system clipboard, takes dropped files, keeps the back and
forward list, follows the navigations a page asks for, keeps its timers
running while nothing moves, and restarts a renderer that died. Its methods
cover what a toolbar needs: `navigate`, `back`, `forward`, `reload`,
`zoomIn`, `zoomOut`, `find`, `copy`, `paste`, `selectAll`.

What only the application can decide arrives at the `Listener`, every method
of which has an empty default: the title, address, security state, favicon
and status line to show; `downloadRequested`; `capabilitiesRequested`
(answered with `view.resolveCapability`); `contextMenuRequested` with the link
under the pointer; `newWindowRequested`; `mediaRequested`,
`windowActionRequested` and `audioCommands`. Keys the window has bound itself
(in its root pane's `WHEN_IN_FOCUSED_WINDOW` input map) never reach the page.

`examples/BrowserViewExample.java` is a complete window in sixty lines. Input
methods work through Swing's own composition window; composing text in place
inside the page is not done.

## What the Java shell does not do

- **Linked popup windows.** The Southstar renderer has no channel for one
  page's `window.open()` to become a second window that stays scripted
  together with its opener (`window.opener`, `postMessage` across windows).
  `window.open()` behaves as it does in the GTK shell.
- **In-process pages.** There is no JNI path: the engine never runs inside the
  JVM.

## Publishing locally

```sh
mvn install                  # → ~/.m2/repository, org.southstar:southstar-java
gradle publishToMavenLocal   # the same coordinates, from Gradle
```

Both builds emit the runnable jar plus `-sources` and `-javadoc`, with the
same manifest (`Main-Class`, `Automatic-Module-Name`) and the licence texts
(`License.md`, `COPYING`, `THIRD-PARTY-LICENSES.md`) under `META-INF/`.
`java/pom.xml` is the single source of truth for the version, which
`scripts/set-version.py` keeps equal to the Cargo workspace's; `build.gradle`
reads it back so the two builds never drift. The jar holds no renderer: ship
`southstar-renderer` (and `southstar-audio`) beside it, or point
`SOUTHSTAR_RENDERER` at one.
