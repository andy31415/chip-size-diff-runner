# branch_diff

A CLI tool to automate building and comparing [connectedhomeip](https://github.com/project-chip/connectedhomeip) (Matter) application binaries across different source revisions (bookmarks/tags), primarily for binary size analysis.

## Overview

`branch_diff` streamlines the workflow of checking how code changes affect binary size:
1.  **Build**: Automatically resolves the current `jj` bookmark or commit, builds the target (on Host or via Podman), and archives the ELF artifact.
2.  **Compare**: Interactively selects two versions of an application and runs the project's size diffing tools, optionally piping to `csvlens` for a rich TUI experience.

## Installation

Ensure you have the following dependencies installed:
- [Rust](https://www.rust-lang.org/) (latest stable)
- [jj (Jujutsu)](https://github.com/martinvonz/jj)
- [uv](https://github.com/astral-sh/uv) (for running Python diff scripts)
- [csvlens](https://github.com/pvolok/csvlens) (optional, for enhanced comparison viewing)
- [Podman](https://podman.io/) (if building non-linux-x64 targets)

Build and install the binary:
```bash
cargo install --path .
```

## Usage

```bash
branch_diff [OPTIONS] <COMMAND>
```

### Global Options

| Option | Default | Description |
|---|---|---|
| `-w, --workdir <PATH>` | `~/devel/connectedhomeip` | Path to the Matter SDK checkout. Must contain `scripts/activate.sh`. |
| `-l, --log-level <LEVEL>` | `info` | Log verbosity: `off`, `error`, `warn`, `info`, `debug`, `trace`. |

---

### `build` — Build an application at a tag

```bash
branch_diff build [OPTIONS] [APPLICATION]
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
branch_diff compare [FROM_FILE] [TO_FILE] [-- EXTRA_DIFF_ARGS...]
```

Compares two ELF binaries using `scripts/tools/binary_elf_size_diff.py`.

| Argument | Description |
|---|---|
| `FROM_FILE` | Baseline artifact path (absolute or relative to workdir). |
| `TO_FILE` | Comparison artifact path (absolute or relative to workdir). |
| `EXTRA_DIFF_ARGS` | Arguments passed to the diff script (after `--`). |

**Interactive Mode**:
If paths are omitted, the tool scans `out/branch-builds/` for ELF files and provides:
1.  **Application Selection**: List of all unique apps found in the builds directory.
2.  **Baseline Selection**: List of tags available for that app, sorted by newest first.
3.  **Comparison Selection**: List of remaining tags for comparison.

**Enhanced Viewing**:
If `csvlens` is installed, the output is formatted as CSV and piped to `csvlens` with pre-configured column filters (`Function`, `Size`, `Type`) for an optimized review experience.

---

## Configuration & Persistence

`branch_diff` maintains state in `~/.cache/branch_diff/session.toml`.

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
