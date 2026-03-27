# browser-cli

A browser session tool for the command line and AI agents. It uses a Chrome/Firefox extension plus Native Messaging to turn live pages into structured XML/JSON, while supporting interactions such as click and type.

For the Chinese version, see [README.md](README.md).

**Highlights:**
- Stateful sessions: login state, cookies, form data, and navigation history are preserved
- Structured XML output: low token cost and easy for AI/agents to consume
- Short element IDs: interact without CSS selectors
- High-fidelity interaction: simulated mouse movement and keyboard typing
- Declarative plugins: reusable TOML automation rules
- Local-only transport: the whole pipeline stays on your machine

---

## Contents

1. [Installation](#installation)
2. [Usage](#usage)
3. [Development](#development)
4. [Why browser-cli](#why-browser-cli)

---

## Installation

### 1. Load the extension

**Chrome:**

Open `chrome://extensions`, enable Developer Mode, click "Load unpacked", and select the `extension/` directory for development; or download the `.zip` from [Releases](../../releases) and load it the same way.

Save the extension ID, which looks like `abcdefghijklmnopabcdefghijklmnop`. You need it for Native Messaging registration.

**Firefox:**

Download the `.xpi` from [Releases](../../releases). Open `about:addons`, click the gear icon, choose "Install Add-on From File...", and select the `.xpi`.

### 2. Install the CLI

**macOS / Linux:**

```sh
curl -fsSL https://raw.githubusercontent.com/4fuu/open-browser-cli/main/install.sh | sh
```

**Homebrew:**

```sh
brew tap 4fuu/open-browser-cli https://github.com/4fuu/open-browser-cli
brew install browser-cli
```

**Windows (Scoop):**

```powershell
scoop bucket add 4fuu https://github.com/4fuu/open-browser-cli
scoop install browser-cli
```

<details>
<summary>Windows (PowerShell script)</summary>

```powershell
irm https://raw.githubusercontent.com/4fuu/open-browser-cli/main/install.ps1 | iex
```

</details>

### 3. Register the Native Messaging host

**Chrome:**

```bash
browser-cli setup --extension-id <extension-id>
```

**Firefox:**

```bash
browser-cli setup --browser firefox
```

After the manifest is written, restart the browser. To uninstall:

```bash
browser-cli teardown --browser chrome   # or --browser firefox
```

---

## Usage

### Basic flow

```bash
# Open a page and get a session ID
browser-cli open https://example.com
# -> Session s1234567890 opened: https://example.com

# Inspect the structured page
browser-cli page s1234567890

# Click an element (e1 means element 1 from page output)
browser-cli click s1234567890 1

# If the target is a link, open it in a new session and keep the current page unchanged
browser-cli click s1234567890 1 --new-session

# Type into an input
browser-cli type s1234567890 3 "hello world"

# Close the session
browser-cli close s1234567890
```

### Command quick reference

```text
browser-cli open <url>
browser-cli list
browser-cli close <session-id>
browser-cli close --all

browser-cli page <session-id> [-p <page>] [--next] [--prev] [--fresh] [--json]
browser-cli click <session-id> <element-id> [-p <page>] [--new-session]
browser-cli type <session-id> <element-id> <text> [-p <page>]
browser-cli search <session-id> <query>
browser-cli text <session-id> <text-id> [-p <page>]
browser-cli wait <session-id> [--selector <css-selector>] [--timeout <ms>]

browser-cli plugin list
browser-cli plugin run <name> <session-id>

browser-cli setup [--browser chrome|firefox] [--extension-id <id>]
browser-cli teardown [--browser chrome|firefox]
```

### Page output

```xml
<page url="https://example.com" title="Example" current="1" total="3">
  <heading level="1">Welcome</heading>
  <text id="t1">This is a long piece of text...</text>
  <link id="e1" href="/login">Sign In</link>
  <button id="e2">Get Started</button>
  <input id="e3" type="text" placeholder="Search..."/>
  <checkbox id="e4" checked/>
  <list>
    <item>Item one</item>
    <item>Item two</item>
  </list>
</page>
```

- `e1`, `e2`, ... are interactive element IDs for `click` and `type`
- `t1`, `t2`, ... are IDs for truncated text blocks, readable with `text`
- `--next` and `--prev` paginate relative to the current scroll position
- `--fresh` bypasses the Relay cache and fetches a fresh browser snapshot
- `click --new-session` only works for links with an `href`; the CLI resolves relative links against the current page URL and opens a brand new session

### Plugins

Plugin rules are TOML files under `~/.config/browser-cli/plugins/`:

```toml
name = "skip-cookie-banner"
description = "Automatically dismiss cookie banners"
match = "*.example.com/*"
trigger = "on_load"

[[steps]]
wait = "Accept"
timeout = 3000
action = "click"
```

### Notes

- Relay always listens on `127.0.0.1:12899`, and only one instance should be running
- Element IDs are re-assigned on every `page`, so fetch the current page before acting
- Use `page --fresh` when dynamic pages need a cache bypass
- `click --new-session` is explicit; normal `click` keeps its current behavior
- If `--new-session` is used on a non-link element, the command fails instead of falling back to an in-place click

---

## Development

**CLI (Rust):**

```bash
cargo build --release
# binary: target/release/browser-cli
```

**Browser extension:**

```bash
cd extension
npm install
npm run build
npm run pack
```

---

## Why browser-cli

### Stateful sessions instead of stateless scraping

Sessions are opened and closed explicitly. As long as a session exists, browser state stays alive: authentication, cookies, filled forms, and navigation history.

### Structured page views instead of raw HTML

Pages are returned as compact XML rather than raw HTML source. Only visible text and actionable elements are kept, which makes the output much easier for AI systems to consume.

### Direct interaction through short IDs

Every `page` response assigns short IDs such as `e1`, `e2`, and `t1`. You interact with these IDs directly rather than building fragile selectors.

### Fast reads via Relay cache

Read operations such as `page`, `search`, and `text` usually hit the Relay cache. Browser round-trips are mainly needed for writes such as `click` and `type`.

### Open links without mutating the current session

For link elements, `click --new-session` opens the target URL in a brand new browser session. This is useful when you want to inspect a destination page without losing the original page state.

### Local-only architecture

The full path is local: CLI -> Relay -> Native Messaging -> extension -> content script. No remote browser service is involved.
