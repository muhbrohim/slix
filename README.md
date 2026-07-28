# Slix

Spec-driven fixed-length message toolkit, implemented as one Rust executable.

Slix detects a message's service code, loads the matching specification, and
shows parsed fields without Python, pip, virtual environments, or installation
steps.

## Status

Slix is under active development. Existing Slicer `.spec` files are supported,
but Slix's machine-readable output uses its own versioned schema.

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
stored outside this repository under the platform configuration directory,
normally `~/.config/slix/projects/`.

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

## Neovim

Bare `slix` starts an empty Neovim buffer with automatic, debounced parsing.
Each non-empty message line receives its own result panel.

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

## License

MIT
