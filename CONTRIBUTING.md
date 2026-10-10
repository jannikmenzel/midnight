# Contributing to Midnight

Thanks for your interest in improving Midnight! Bug reports, ideas and pull requests are welcome.

## Before you start

- For larger changes, please open an issue first so we can agree on the approach.
- Never attach real invoices, receipts, tax data or other personal information to issues or pull requests.

## Setup

Requirements: Node 20+, and on macOS the Xcode Command Line Tools. The Rust version is pinned in
`rust-toolchain.toml` and installed automatically by `rustup`. LaTeX (`xelatex` from MacTeX or TeX Live) is required
to create invoice and timesheet PDFs.

```bash
npm install
npm run tauri dev
```

To keep your real bookkeeping data separate while developing, the MCP server honours `MIDNIGHT_DATA_DIR`.

## Checks

Please make sure these pass before opening a pull request. CI runs the same checks.

```bash
npm run build
cd src-tauri
cargo clippy --all-targets -- -D warnings
cargo test
```

The third-party license notices bundled with the app are generated during the release build. To inspect them
locally, install [cargo-about](https://github.com/EmbarkStudios/cargo-about) and run:

```bash
cargo install cargo-about --locked --features cli
scripts/third_party_notices.sh
```

## License

By contributing, you agree that your contributions are licensed under the [AGPL-3.0](LICENSE).
