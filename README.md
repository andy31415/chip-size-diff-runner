# chip-size

A CLI tool to automate building and comparing [connectedhomeip](https://github.com/project-chip/connectedhomeip) (Matter) application binaries across different source revisions (bookmarks/tags), primarily for binary size analysis.

## Overview

`chip-size` streamlines the workflow of checking how code changes affect binary size:
1.  **Build**: Automatically resolves the current `jj` bookmark or commit, builds the target (on Host or via Podman), and archives the ELF artifact.
2.  **Compare**: Interactively selects two versions of an application and runs the project's size diffing tools, optionally piping to `csvlens` for a rich TUI experience.

## Installation

Ensure you have the following dependencies installed:
- [Rust](https://www.rust-lang.org/) (latest stable)
- [jj (Jujutsu)](https://github.com/martinvonz/jj)
- [uv](https://github.com/astral-sh/uv) (for running Python diff scripts)
- [visidata](https://www.visidata.org/) or [csvlens](https://github.com/pvolok/csvlens) (optional, for enhanced comparison viewing)
- [Podman](https://podman.io/) (if building non-linux-x64 targets)

Install directly from GitHub:
```bash
cargo install --git https://github.com/andy31415/chip-size-diff-runner.git
```

Or, if you have a local clone, build and install from the source tree:
```bash
cargo install --path .
```

## Usage

```bash
chip-size [OPTIONS] <COMMAND>
```

### Global Options

| Option | Default | Description |
|---|---|---|
| `-w, --workdir <PATH>` | `~/devel/connectedhomeip` | Path to the Matter SDK checkout. Must contain `scripts/activate.sh`. |
| `-l, --log-level <LEVEL>` | `info` | Log verbosity: `off`, `error`, `warn`, `info`, `debug`, `trace`. |

---

### `build` — Build an application at a tag

```bash
chip-size build [OPTIONS] [APPLICATION]
```

Builds the application and stores it in `out/branch-builds/<TAG>/<APP_PATH>`.

| Argument/Option | Description |
|---|---|
| `APPLICATION` | Build target name. If omitted, an interactive fuzzy-find list is shown. |
| `-t, --tag <TAG>` | Custom tag for the build. If omitted, inferred from `jj`. |

**Tag Inference Strategy**:
1.  If `--tag` is provided, use it.
2.  If the `jj` working copy is clean, use the bookmark name at `@-`.
3.  Otherwise, prompt for:
    *   The short commit ID of the current change.
    *   A list of recent `jj` bookmarks.
    *   A custom manual entry.

**Execution Environment**:
- Targets starting with `linux-x64-` are built on the **Host** via `bash`.
- All other targets are built via **Podman** in the `bld_vscode` container.

---

### `compare` — Compare two build artifacts

```bash
chip-size compare [OPTIONS] [FROM_FILE] [TO_FILE] [-- EXTRA_DIFF_ARGS...]
```

Compares two ELF binaries using `scripts/tools/binary_elf_size_diff.py`.

| Argument/Option | Description |
|---|---|
| `FROM_FILE` | Baseline artifact path (absolute or relative to workdir). |
| `TO_FILE` | Comparison artifact path (absolute or relative to workdir). |
| `--viewer <VIEWER>` | Viewer tool for the CSV output (see below). Default: `default`. |
| `EXTRA_DIFF_ARGS` | Arguments passed directly to the diff script (after `--`). |

**Interactive Mode**:
If paths are omitted, the tool scans `out/branch-builds/` for ELF files and provides:
1.  **Application Selection**: List of all unique apps found in the builds directory.
2.  **Baseline Selection**: List of tags available for that app, sorted by newest first.
3.  **Comparison Selection**: List of remaining tags for comparison.

**Viewer Options** (`--viewer`):

| Value | Behaviour |
|---|---|
| `default` | Auto-detect: use `vd` if available, then `csvlens`, otherwise print a plain table. |
| `vd` / `visidata` | Pipe CSV to `vd -`. |
| `csvlens` | Pipe CSV to `csvlens` with pre-configured column filters (`Function`, `Size`, `Type`). |
| `custom:<cmd>` | Pipe CSV to an arbitrary command. Arguments are supported, e.g. `custom:"grep chip"`. |

---

**Diff Engine Options** (`--diff-engine`):

| Value | Description |
|---|---|
| `native` | **(Default)** Uses a native Rust implementation with the `elf` crate to parse symbols. Recommended. |
| `nm` | Uses the system's `nm` tool to extract symbols. Output format may vary. |
| `goblin` | Uses the `goblin` crate for ELF parsing. |
| `script` | Uses the original Python script (`scripts/tools/binary_elf_size_diff.py`) from the Matter SDK. Requires `uv` to be installed. |

---

## Configuration & Persistence

`chip-size` maintains state in `~/.cache/chip-size/session.toml`.

Stored data includes:
-   **Last Workdir**: Used as the default if `-w` is not provided.
-   **Recent Applications**: The most frequently built targets appear at the top of the selection list.
-   **Last Comparison**: Remembers the last `from` and `to` files for quick re-comparison.
-   **Default Targets**: A list of common build targets shown as fallbacks. You can manually edit `session.toml` to customize this list.

## Project Structure

- `src/domain/`: Core logic for artifact discovery and VCS (jj) interaction.
- `src/runner/`: Low-level process execution for builds and diffs.
- `src/ui/`: Interactive `skim`-based fuzzy finder.
- `src/commands/`: Command orchestration and CLI argument handling.
- `src/persistence.rs`: Session state management.
