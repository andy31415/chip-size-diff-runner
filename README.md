# branch_diff

A CLI tool to build and compare [connectedhomeip](https://github.com/project-chip/connectedhomeip) application binaries across different source revisions, primarily for binary size analysis.

## Usage

```
branch_diff [OPTIONS] <COMMAND>
```

### Global options

| Option | Default | Description |
|---|---|---|
| `--workdir <PATH>` | `~/devel/connectedhomeip` | Path to the connectedhomeip checkout. Must contain `scripts/activate.sh`. |
| `--log-level <LEVEL>` | `info` | Log verbosity: `off`, `error`, `warn`, `info`, `debug`, `trace`. |

### `build` — Build an application at a tag

```
branch_diff build [OPTIONS] <APPLICATION>
```

Builds the specified application target and copies artifacts to `out/branch-builds/<TAG>/` within the workdir.

| Argument | Description |
|---|---|
| `<APPLICATION>` | Build target name (e.g., `linux-x64-all-clusters-app`). |
| `--tag <TAG>` | Tag for the output directory. If omitted, inferred from the jj repository. |

**Tag inference:** If `--tag` is not given and the jj working copy is clean, the bookmark at `@-` is used. Otherwise, an interactive prompt lets you choose the current commit ID, a recent bookmark, or enter a custom tag.

**Execution:** `linux-x64-*` targets build on the host; all other targets run inside a `podman exec bld_vscode` container.

### `compare` — Compare two build artifacts

```
branch_diff compare [FROM_FILE] [TO_FILE] [-- EXTRA_DIFF_ARGS...]
```

Runs `scripts/tools/binary_elf_size_diff.py` (via `uv run`) to compare two ELF binaries.

| Argument | Description |
|---|---|
| `FROM_FILE` | Baseline artifact path (absolute or workdir-relative). |
| `TO_FILE` | Comparison artifact path (absolute or workdir-relative). |
| `EXTRA_DIFF_ARGS` | Extra arguments forwarded to the diff script. Defaults to `--output table`. |

**Interactive mode:** If either path is omitted, the tool scans `out/branch-builds/` for ELF binaries and presents fuzzy-find prompts (via `skim`) to select the application and tags to compare. Previous selections are remembered and shown at the top of the list.

Selections are persisted to `~/.cache/branch_diff/defaults.toml`.
