# Contributing

## Requirements

- Rust 1.85 or newer
- Git
- Neovim for interactive editor testing

## Local checks

Run the same checks required by CI:

```powershell
cargo fmt --all --check
cargo clippy --all-targets --all-features --locked -- -D warnings
cargo test --all-targets --all-features --locked
cargo build --release --locked
```

Keep changes focused, add tests for behavior changes, and do not commit customer
messages or enterprise specification files.

## Release process

1. Update `CHANGELOG.md` and the version in `Cargo.toml`.
2. Run all local checks.
3. Merge the release preparation into `main` and confirm CI succeeds.
4. Create and push an annotated `vX.Y.Z` tag matching `Cargo.toml`.
5. Verify the GitHub Release assets and `SHA256SUMS`.
6. Update and test the Scoop manifest using the published Windows ZIP hash.

The release workflow rejects tags that do not exactly match the package version.
