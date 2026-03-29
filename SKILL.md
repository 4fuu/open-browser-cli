---
name: open-browser-cli
description: "Drives a real browser session via browser-cli: open stateful sessions, inspect structured XML/JSON pages, search content, click, type, wait for page stability, read truncated text or paginated blocks, capture screenshots, download browser-accessible resources, and run plugins. Use when login state, cookies, SPA rendering, or durable page interactions matter."
---

# browser-cli

`browser-cli` is a session-based browser control tool for CLI and AI agents. It is not a stateless scraper. Open a session, inspect the current structured page, then act on the current page IDs.

## Core rules

- Always fetch the current page before interacting. Element IDs are reassigned on every `page`.
- `click` and `type` accept either a full element ID (`e3`), the numeric part (`3`), or a text query matched against the current page's interactive elements.
- `text` and `block` accept either full IDs such as `t1` / `b1` or their numeric parts (`1`).
- Prefer `--json` when another tool or agent will consume the result.
- Use `--verbose` with `page`, `search`, `block`, or `view` when you need full JSON detail instead of the default compact form.
- Use `--fresh` when the cache may be stale or the page is highly dynamic.
- `open`, `click`, `type`, and `wait` return the current page by default; use `--quiet` when you only need a compact success result.
- Use `click --new-session` only for link elements with `href`. It opens the destination in a new session and keeps the source session unchanged.

## Default workflow

1. Open a session: `browser-cli open <url>`
2. Inspect the page: `browser-cli page <session_id>`
3. Choose targets from the returned `element_id`, `text_id`, or `block_id`
4. Interact with `click`, `type`, or use `wait` to let the page settle
5. Re-run `page` after any meaningful state change
6. Close the session when done: `browser-cli close <session_id>`

## Agent guidance

- Prefer `--json` for chaining multiple tool calls.
- Prefer `search` before `page` only when you need a quick target lookup and not full page context.
- Prefer plain `wait` after actions that trigger client-side rendering or delayed navigation; add `--for <text>` when you are waiting for a specific control or label to appear.
- If a flow spans multiple tabs or destinations, preserve the current state by opening links with `click --new-session`.
- Treat this tool as stateful browser automation with structured output, not as HTML scraping.

## Pause for user confirmation

Stop and wait for the user when the browser flow crosses an authentication, consent, or high-risk boundary. `browser-cli` preserves real session state, so continuing blindly can leak secrets or trigger actions the user did not intend.

- Pause when the page clearly requires login, sign-in, SSO selection, passkey use, 2FA, email/SMS verification, CAPTCHA, or any other human identity check.
- Pause before entering secrets unless the user explicitly asked for that exact step and already provided the credential material through a trusted channel.
- Pause before submitting actions with external side effects such as purchase, payment, transfer, delete, publish, send, authorize, install, connect account, or accept legal terms.
- Pause when the next click is ambiguous and could either continue navigation or commit a real account change.
- Pause when a site asks the user to solve a challenge outside normal DOM automation, such as a hardware key, QR login, native app approval, or anti-bot checkpoint.

When you pause:

- Tell the user exactly what blocked the flow and what page or control triggered the stop.
- Ask the user to complete the sensitive step manually in the browser if appropriate.
- After the user confirms they are done, refresh context with `browser-cli page <session_id> --fresh` before taking any further action.

## Use this skill when

- The task needs a real browser tab with preserved login state, cookies, history, or form state.
- The page is rendered client-side and raw HTTP is not enough.
- You need low-noise XML/JSON instead of full HTML.
- You need follow-up actions such as `click`, `type`, `wait`, `search`, `text`, `block`, `view`, `screenshot`, `download`, or `plugin run`.

## Do not use this skill for

- Simple API calls or static page fetches.
- Pixel-accurate visual diff workflows.
- Acting on stale element IDs from an older `page` result.

## Preconditions

- The browser extension must already be loaded.
- Native Messaging must already be registered:
  - Chrome: `browser-cli setup --extension-id <extension-id>`
  - Firefox: `browser-cli setup --browser firefox`
- The extension launches `browser-cli relay` through Native Messaging. Users normally do not run `relay` directly.
- If `browser-cli` is not installed or the browser extension is not configured, see <https://github.com/4fuu/open-browser-cli> for installation and setup instructions.

## High-value commands

### Session management

```bash
browser-cli open https://example.com
browser-cli open https://example.com --quiet
browser-cli list
browser-cli close s123
browser-cli close --all
```

### Inspect the current page

Default output is XML. Use `--json` for structured machine consumption. `page`, `search`, `block`, and `view` support `--verbose` to return the full JSON payload instead of the default compact form.

```bash
browser-cli page s123
browser-cli page s123 -p 2
browser-cli page s123 --next
browser-cli page s123 --prev
browser-cli page s123 --fresh
```

### Find targets before acting

`search` is the fastest way to locate likely controls or text. It can return `page` and `element_id` for actionable matches.

```bash
browser-cli search s123 "sign in"
browser-cli search s123 "search" --fresh --json
browser-cli search s123 "search" --fresh --json --verbose
```

If page text is truncated, fetch the full text by `text_id`:

```bash
browser-cli text s123 t1
browser-cli text s123 1
```

If a list or table is block-paginated, continue reading with `block`:

```bash
browser-cli block s123 b1 --source-page 1 -p 2
browser-cli block s123 1 --source-page 1 -p 2
browser-cli block s123 b1 --all
browser-cli block s123 b1 --all --json --verbose
```

For a focused subtree or full expansion of one target, use `view`:

```bash
browser-cli view s123 e3
browser-cli view s123 t1
browser-cli view s123 "pricing"
browser-cli view s123 e3 --json --verbose
```

By default, `view` narrows list/table matches to the single `item` or `row` containing the target element. Add `--verbose` to keep the full surrounding list/table context.

### Capture or retrieve assets

Use `screenshot` for the current viewport and `download` for browser-accessible files or media URLs.

```bash
browser-cli screenshot s123
browser-cli screenshot s123 --output hero.png
browser-cli screenshot s123 --quality 85 --json
browser-cli download s123 e7
browser-cli download s123 "https://example.com/file.pdf" --output file.pdf
browser-cli download s123 e7 --json
```

### Interact with the page

Always resolve targets from the latest `page` output first.

```bash
browser-cli click s123 1
browser-cli click s123 "Sign in"
browser-cli click s123 1 --new-session
browser-cli type s123 3 "hello world"
browser-cli type s123 "Search" "hello world"
browser-cli wait s123 --timeout 5000
browser-cli wait s123 --for "Continue" --json
browser-cli click s123 1 --json
```

Use `--quiet` for automation flows that only need success/failure:

```bash
browser-cli click s123 1 --quiet
browser-cli type s123 3 "hello world" --quiet
```

## Page interpretation notes

- Interactive elements are exposed as short IDs like `e1`, `e2`, `e3`; `click` / `type` accept either `e3` or `3`.
- Long text may be truncated in-page and assigned `t1`, `t2`, etc.; `text` accepts either `t1` or `1`.
- Large lists or tables may be exposed as block IDs `b1`, `b2`, etc.; `block` accepts either `b1` or `1`.
- Pagination is viewport-based. `page -p N` reads a logical page slice without scrolling the browser manually.

## Plugin usage

Plugin rules live under `~/.config/browser-cli/plugins/` and are written in TOML. Use plugins for repeatable site-specific flows such as dismissing cookie banners or auto-filling forms.

```bash
browser-cli plugin list
browser-cli plugin run <name> s123 --json
```

When authoring or debugging plugin rules, remember that string waits are matched against structured page content.

## Failure patterns

- `session not found`: the session was closed or never created.
- Element lookup failed: you used an ID from an old page snapshot or the wrong `-p` page number.
- No interactive element matched the query: retry with `page` or `search` to confirm the visible label, placeholder, or link text.
- `--new-session` failed: the chosen element is not a link with `href`.
- Dynamic page mismatch: retry with `page --fresh`, `search --fresh`, or action flags like `click --fresh`.
