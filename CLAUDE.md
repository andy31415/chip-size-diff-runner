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
- **Artifact discovery** (`selector.rs` / `BuildArtifacts::find`): Walks `out/branch-builds/` in the workdir, reads every file and uses `goblin` to confirm it's an ELF. Builds a `BTreeMap<app_path, Vec<(tag, SystemTime)>>` sorted newest-first per app.
- **Typed selection** (`selector.rs`): `SelectItem` trait with a `display_text()` method. `AppItem` and `TagItem` implement it with column-aligned formatting (timestamps included). The generic `select<T: SelectItem>(prompt, items, default_index)` places the default at position 0, runs skim, then recovers the original `T` by exact `display_text()` match — no string parsing. `String` also implements `SelectItem` for plain lists (e.g. build targets). `create_tag_items(entries)` computes column width over a given slice so filtered subsets stay aligned.
- **Defaults persistence** (`defaults.rs`): `~/.cache/branch_diff/defaults.toml` stores `workdir`, `from_file`, `to_file`, and `recent_applications` (last 10 build targets). Loaded on startup, saved after each successful operation. The default item in each skim prompt is found by position (`items.iter().position(|i| i.field == stored_value)`).
- **Defaults persistence** (`defaults.rs`): `~/.cache/branch_diff/defaults.toml` stores `workdir`, `from_file`, `to_file` as relative paths (relative to workdir). Loaded on startup, saved after each successful operation.
- **Workdir validation**: Must contain `scripts/activate.sh` — this is the connectedhomeip activation script used to source the build environment.
- **Build dispatch**: `linux-x64-*` targets run locally via `bash -c`; all other targets run inside `podman exec -w /workspace bld_vscode`.
- **Compare argument format**: Paths passed on CLI can be absolute or workdir-relative; `normalize_path_str` strips the workdir prefix to store them as relative paths. `parse_artifact_path` expects `out/branch-builds/<tag>/<app_path>`.
