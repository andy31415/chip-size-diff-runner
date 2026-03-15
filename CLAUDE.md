# CLAUDE.md

This file provides guidance for AI assistants working with this repository.

## Commands

```bash
cargo build                  # build
cargo run -- <args>          # run
cargo test                   # run all tests
cargo clippy                 # lint
cargo fmt                    # format
```

## Architecture

`chip-size` is a Rust CLI tool designed to build and compare binaries (specifically for the [connectedhomeip](https://github.com/project-chip/connectedhomeip) project) across different source revisions (jj bookmarks/tags) for size and difference analysis.

### Project Structure

- **`src/main.rs`**: CLI entry point and orchestration.
- **`src/commands/`**: Subcommand handlers (`build`, `compare`).
- **`src/domain/`**: Core business logic and data structures.
    - `artifacts.rs`: ELF discovery, path parsing, and `BuildArtifacts` management.
    - `vcs.rs`: Version control system interaction (specifically `jj`).
- **`src/runner/`**: Execution layer for external processes.
    - `build_engine.rs`: Logic for Host vs. Podman build dispatch.
    - `diff_engine.rs`: Logic for running `binary_elf_size_diff.py` and piping to a viewer. Contains `ViewerTool` (auto-detect, visidata, csvlens, custom).
    - `process.rs`: `CommandChain` utility for command piping.
- **`src/ui/`**: User interface components.
    - `fuzzy.rs`: Generic `skim` wrapper and the `SelectItem` trait for interactive selection.
- **`src/persistence.rs`**: Manages `SessionState` (stored in `~/.cache/chip-size/session.toml`).

### Data Flow

**Build Subcommand**: 
`main.rs` → `commands/build.rs` → `domain/vcs.rs` (resolves tag) → `runner/build_engine.rs` (dispatches to `bash` or `podman exec`) → `scripts/build/build_examples.py`.

**Compare Subcommand**:
`main.rs` → `commands/compare.rs` → `ui/fuzzy.rs` (to pick app + tags) → `runner/diff_engine.rs` (runs size diff script) → optional pipe to viewer (`vd`, `csvlens`, or custom).

### Key Design Details

- **Tag Resolution** (`domain/vcs.rs`): Automatically uses a clean bookmark at `@-` if available; otherwise, prompts via `skim` with options for current commit ID, recent bookmarks, or custom entry.
- **Artifact Discovery** (`domain/artifacts.rs`): Recursively walks `out/branch-builds/` (defined by `BUILDS_PATH_PREFIX`), validates ELF headers using `goblin`, and groups them by application path and tag.
- **Path Format** (`domain/artifacts.rs`): `BUILDS_PATH_PREFIX` is the single source of truth for the `out/branch-builds/<tag>/<app>` format, shared by `build_path()` and `parse_artifact_path()` in the compare command.
- **Fuzzy Selection** (`ui/fuzzy.rs`): Uses a `SelectItem` trait. `AppItem` and `TagItem` implement this with ANSI-decorated, column-aligned formatting. Selection recovery uses exact `display_text()` matching. Default-item reordering is handled by `build_ordered_indices()`.
- **Viewer Selection** (`runner/diff_engine.rs`): `ViewerTool` controls how diff output is displayed. `default` auto-detects `vd` then `csvlens` then falls back to plain table. `custom:<cmd>` pipes CSV to an arbitrary command (arguments supported, e.g. `custom:"grep chip"`).
- **Session Persistence** (`persistence.rs`): Stores `workdir`, `recent_applications`, `default_targets`, and last-used comparison files. Paths are stored relative to the `workdir`. `load_from(path)` / `save_to(path)` variants allow testing without `dirs::cache_dir()`.
- **Environment Requirements**: The `workdir` must contain `scripts/activate.sh`.
- **Build Dispatch**: `linux-x64-*` targets run locally; others run via `podman` in the `bld_vscode` container.
