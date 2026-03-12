# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Commands

```bash
cargo build                  # build
cargo run -- <args>          # run
cargo test                   # run all tests
cargo test <test_name>       # run a single test
cargo clippy                 # lint
cargo fmt                    # format
```

## Architecture

This is `branch_diff`, a Rust CLI tool that builds and compares [connectedhomeip](https://github.com/project-chip/connectedhomeip) ELF binaries across jj bookmarks for size diff analysis.

### Data flow

**build subcommand**: `main.rs` → `commands/build.rs` → `tag_generator.rs` (resolves jj bookmark) → spawns `bash` or `podman exec bld_vscode` running `scripts/build/build_examples.py --target <app> --copy-artifacts-to out/branch-builds/<tag>/`

**compare subcommand**: `main.rs` → `commands/compare.rs` → `selector.rs` (skim fuzzy UI to pick app + two tags) → spawns `uv run scripts/tools/binary_elf_size_diff.py`

### Key design details

- **Tag resolution** (`tag_generator.rs`): If `--tag` not given, checks if jj working copy is clean and reads the bookmark at `@-`. If dirty or no bookmark, falls back to interactive skim selection (commit ID, custom, or any listed bookmark).
- **Artifact discovery** (`selector.rs` / `BuildArtifacts::find`): Walks `out/branch-builds/` in the workdir, reads every file and uses `goblin` to confirm it's an ELF. Builds a `BTreeMap<app_path, Vec<tag>>`.
- **Interactive selection** (`selector.rs`): All fuzzy prompts go through `skim`. The previously selected item is moved to position 0 so it's the default. ESC / empty selection returns an `Err`.
- **Defaults persistence** (`defaults.rs`): `~/.cache/branch_diff/defaults.toml` stores `workdir`, `from_file`, `to_file` as relative paths (relative to workdir). Loaded on startup, saved after each successful operation.
- **Workdir validation**: Must contain `scripts/activate.sh` — this is the connectedhomeip activation script used to source the build environment.
- **Build dispatch**: `linux-x64-*` targets run locally via `bash -c`; all other targets run inside `podman exec -w /workspace bld_vscode`.
- **Compare argument format**: Paths passed on CLI can be absolute or workdir-relative; `normalize_path_str` strips the workdir prefix to store them as relative paths. `parse_artifact_path` expects `out/branch-builds/<tag>/<app_path>`.
