# Slix

Spec-driven fixed-length message toolkit, distributed as one Rust executable.

Slix detects a message's service code, loads the matching specification, and
shows parsed fields without Python, pip, or virtual environments.

## Status

Slix is under active development. Existing Slicer `.spec` files are supported,
but Slix's machine-readable output uses its own versioned schema. Releases in
the `0.1.x` series may change CLI and schema behavior.

Supported release targets are 64-bit Windows and 64-bit Linux. Neovim is only
required for interactive editing; parsing and project commands work without it.

## Install

### Scoop (Windows)

After the first release is published:

```powershell
scoop bucket add muhbrohim https://github.com/muhbrohim/scoop-bucket
scoop install muhbrohim/slix
slix --version
```

### Release archive

Download the archive for your platform from
[GitHub Releases](https://github.com/muhbrohim/slix/releases), verify it against
`SHA256SUMS`, extract it, and place `slix` or `slix.exe` on `PATH`.

## Commands

```text
slix                         Open a live Neovim parsing session
slix parse [FILE]            Parse a file or standard input
slix list                    List and search specifications
slix open [SERVICE_CODE]     Open a spec or the spec directory
slix project                 Show the active project
slix project import LHB DIR  Import a project
slix project use LHB         Select a project
slix doctor                  Check the environment
slix stats                   Show project statistics
```

Each non-empty input line is one message. Real enterprise specifications are
stored outside this repository under the platform configuration directory.

| Platform | Default configuration directory |
| --- | --- |
| Windows | `%APPDATA%\slix` |
| Linux | `$XDG_CONFIG_HOME/slix` or `~/.config/slix` |

Set `SLIX_CONFIG_HOME` to override the configuration directory. Select a
project per invocation with `--project NAME` or through the `SLIX_PROJECT`
environment variable.

Parsing outputs clean nested JSON by default. Use `--output table`, `yaml`,
`csv`, or `raw` when another representation is more useful.

## Build

```powershell
cargo build --release --locked
```

The distributable executable is `target\release\slix.exe` on Windows.

## First project

Import an existing Slicer-compatible specification directory once:

```powershell
.\target\release\slix.exe project import LHB C:\path\to\specs
.\target\release\slix.exe project use LHB
.\target\release\slix.exe doctor
```

Slix copies specifications into its managed configuration directory. The
source specifications and imported enterprise data are not committed to this
repository.

The global `config.toml` supports these optional fields:

```toml
active_project = "LHB"
default_output = "json"
editor = "nvim"
```

An imported project's `project.toml` defines its name, text encoding, and specs
directory. Run `slix doctor` to display the resolved paths and validate the
active project.

## Neovim

Bare `slix` starts an empty Neovim buffer with automatic, debounced parsing.
Each non-empty message line receives its own clean JSON result panel. Metadata
appears first, followed by the header and body; scalar body fields precede
repeated collections, and `unparsed_tail` remains last.

For normal Neovim sessions, load the local plugin from `nvim/` and call:

```lua
require("slix").setup({
    cmd = { "C:/path/to/slix.exe" },
    project = "LHB",
})
```

The default mappings preserve the Slicer workflow: `<leader>ss`, `<leader>sb`,
`<leader>sj`, `<leader>sr`, `<leader>sS`, and `<leader>sf`. Slix also adds
`<leader>sd` for field-definition jumps and `<leader>sR` to reload specs.

## Exit codes

| Code | Meaning |
| --- | --- |
| `0` | Command completed successfully |
| `1` | Parsing completed with message diagnostics |
| `2` | Invalid invocation, configuration, I/O, or operational failure |

## Development

See [CONTRIBUTING.md](CONTRIBUTING.md) for local checks and the release process.
Security reports are handled according to [SECURITY.md](SECURITY.md).

## License

MIT
