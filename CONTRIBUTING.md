# Contributing to browser-cli

## Requirements

- Rust stable toolchain
- Node.js 22+
- Chrome or Firefox with the extension loaded

## Development setup

**CLI (Rust):**

```bash
cargo build
cargo test
```

**Browser extension:**

```bash
cd extension
npm install
npm run build
```

After rebuilding the extension, reload it in your browser and use `--fresh` to bypass cached snapshots.

## Before submitting

- Run `cargo test` for any Rust changes.
- Run `npm run typecheck && npm run build` for any extension changes.
- If you changed `RawNode`, `RawSnapshot`, or extension collection attributes, sync both the Rust and TypeScript definitions.
- If you changed page structure or XML output, add or update tests in `src/page/`.

## Pull requests

- Keep changes focused — one concern per PR.
- Describe what changed and why in the PR description.
- Do not add site-specific logic to the core layer; put it in a plugin instead.

## Reporting issues

Open an issue and include:

- `browser-cli --version` output
- Browser name and version
- OS and platform
- Steps to reproduce
