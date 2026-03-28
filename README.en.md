# browser-cli

> Control your real browser from CLI — sessions, login state, and cookies all preserved

[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)
[![Rust](https://img.shields.io/badge/rust-stable-orange.svg)](https://www.rust-lang.org)
[![Platform](https://img.shields.io/badge/platform-macOS%20%7C%20Linux%20%7C%20Windows-lightgrey.svg)]()
[![中文](https://img.shields.io/badge/lang-%E4%B8%AD%E6%96%87-informational)](README.md)

A browser session tool for the command line and AI agents. Uses a Chrome/Firefox extension plus Native Messaging to turn your real browser's live pages into structured XML/JSON, with support for click, type, and other interactions.

**Features:**
- **Stateful sessions** — login state, cookies, form data, and navigation history are preserved
- **Tiny binary** — the CLI binary is about 2 MB, uses very little memory, and is easy to install and use
- **Structured XML output** — low token cost and easy for AI/agents to consume
- **Short element IDs** — interact without CSS selectors
- **High-fidelity interaction** — simulated mouse movement and keyboard typing to avoid bot detection
- **High performance** — read operations hit in-memory cache, DOM is collected in non-blocking batches, and heavy structuring runs on the CLI side
- **Declarative plugins** — reusable TOML automation rules

---

## Comparison

| | browser-cli | [opencli](https://github.com/jackwener/opencli) | Playwright / Selenium | curl / requests |
|---|---|---|---|---|
| Browser | Your real browser (Chrome/Firefox) | Your real Chrome | New isolated instance | No browser |
| Session | ✅ Login / cookies | ✅ Existing Chrome session | ❌ Resets each time | ❌ |
| Coverage | Any page | 50+ preset sites | Any page | Any URL |
| Interaction | General (click / type) | Site-specific commands | Programmatic API | HTTP requests |
| Bot detection | Real fingerprint | Anti-detection built-in | Easily flagged | Easily flagged |
| Page output | Compact XML / JSON | Deterministic JSON | HTML / DOM | HTML |
| AI consumption | Low token, structured | Low token, structured JSON | High token, raw | High token |

---

## Contents

1. [Installation](#installation)
2. [Usage](#usage)
3. [Development](#development)
4. [Why browser-cli](#why-browser-cli)
5. [Contributing](#contributing)

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
scoop bucket add open-browser-cli https://github.com/4fuu/open-browser-cli
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
# Open a page and return the structured page
browser-cli open https://example.com

# Inspect the page structure
browser-cli page s1234567890

# Click an element (e1, 1, or text query all work)
browser-cli click s1234567890 "Sign In"

# Type into an input
browser-cli type s1234567890 "Search" "hello world"

# Wait for the page to settle, or for specific text to appear
browser-cli wait s1234567890
browser-cli wait s1234567890 --for "Continue"

# Read truncated text / paginated blocks
browser-cli text s1234567890 t1
browser-cli block s1234567890 b1 --source-page 1 --all

# Close the session
browser-cli close s1234567890
```

### Command quick reference

```text
browser-cli open <url> [--wait <ms>] [--quiet] [--json]
browser-cli list [--json]
browser-cli close <session-id> [--json]
browser-cli close --all [--json]
browser-cli --version

browser-cli page <session-id> [-p <page>] [--next] [--prev] [--fresh] [--json] [--verbose]
browser-cli click <session-id> <element-id|number|query> [-p <page>] [--new-session] [--fresh] [--quiet] [--json]
browser-cli type <session-id> <element-id|number|query> <text> [-p <page>] [--fresh] [--quiet] [--json]
browser-cli search <session-id> <query> [--fresh] [--json] [--verbose]
browser-cli text <session-id> <text-id|number> [-p <page>] [--fresh] [--json]
browser-cli block <session-id> <block-id|number> [--source-page <page>] [(-p <block-page>)|--all] [--fresh] [--json] [--verbose]
browser-cli view <session-id> <element-id|number|query> [-p <page>] [--fresh] [--json] [--verbose]
browser-cli wait <session-id> [--for <text>] [--timeout <ms>] [--quiet] [--json]

browser-cli plugin list [--json]
browser-cli plugin run <name> <session-id> [--json]

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

- `e1`, `e2`, ... are interactive element IDs for `click` and `type`; both `e1` and `1` are accepted
- `t1`, `t2`, ... are IDs for truncated text blocks, readable with `text`; both `t1` and `1` are accepted
- `b1`, `b2`, ... are IDs for paginated list/table blocks, readable with `block`; both `b1` and `1` are accepted
- `--next` / `--prev` paginate relative to the current scroll position
- `--fresh` bypasses the Relay cache and fetches a fresh browser snapshot
- `--version` prints the version injected at build time, or `unknown` if not set
- `open` returns the page structure by default; use `--quiet` for session info only, `--wait 0` to skip the post-open stability wait
- `open` / `close` / `list` / `search` / `wait` / `plugin` / `view` all support `--json`
- `page` / `search` / `block` / `view` support `--verbose`; this mainly matters for JSON mode, where the default `--json` output is compact and `--verbose` returns full detail
- The `<target>` for `click` / `type` accepts a prefixed ID (`e1`), a bare number (`1` maps to `e1`), or a text query matching button text, link text, or input placeholder/value
- `click` / `type` output the updated full page XML by default; use `--quiet` for a success summary, `--json` for a structured response
- `wait` returns the latest page on success; use `--quiet` in automation pipelines when you only need the success/timeout result
- `search` still returns `page`, `tag`, a context snippet, and `element_id` in plain text mode; in `--json` mode it returns a compact result by default, and the full match structure with `--verbose`
- Truncated text is shown as `[...truncated]`; oversized `list` / `table` blocks are paginated by XML line budget rather than item count — use `block --source-page <page> -p <block-page>` to read a single page, or `--all` to expand the entire block
- `view` returns a focused view of an element, truncated text, or block; target accepts `e3`, `3`, `t1`, `b1`, or a text query
- When `view` targets an element inside a list or table, it returns only the matching `item` / `row` by default; add `--verbose` to keep the full list/table context
- `click --new-session` only works for links with an `href`; the CLI resolves the URL and opens a new session, leaving the current page unchanged

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

- Relay always listens on `127.0.0.1:12899`; only one instance should be running at a time
- Element IDs (`e1`, `e2`, ...) are re-assigned on every `page` — fetch the current page before acting
- `--fresh` on any read command (`page`, `search`, `text`, `block`, `view`) bypasses the cache for dynamic pages; `click` / `type` also accept `--fresh`
- `wait` returns the latest page on success; use `--quiet` in automation pipelines when only the success/timeout signal matters
- `click --new-session` is explicit and does not apply to normal clicks; if the target is not a link, the command fails

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

browser-cli controls your real browser as it runs on your machine. Sessions are opened and closed explicitly — as long as a session is alive, login state, cookies, filled forms, and navigation history are all preserved.

Pages are returned as compact XML, keeping only actionable elements and visible text, which keeps token usage low and output easy for AI agents to consume.

After each `page`, interactive elements get short IDs (`e1`, `e2`, ...) that you use directly, with no fragile CSS selectors or XPath needed. Clicks simulate real mouse movement and inputs are dispatched character-by-character with random delays, bypassing bot detection and React/Vue value checks.

Read commands (`page`, `search`, `text`) hit the Relay in-memory cache and never round-trip to the browser; only write operations (`click`, `type`) trigger browser execution and update the snapshot.

The content script collects the DOM in batches of 100 nodes with 8ms intervals to avoid blocking page rendering, and all heavy structuring work runs on the CLI side. Repetitive actions can be described in TOML plugins that trigger automatically across sessions.

---

## Contributing

Issues and pull requests are welcome. Please read [CONTRIBUTING.md](CONTRIBUTING.md) before submitting.
