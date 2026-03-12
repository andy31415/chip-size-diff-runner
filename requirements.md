# Build and Compare Tool Requirements

This document outlines the requirements for a Rust application designed to automate the building and comparison of application binaries, primarily for size difference analysis. This tool aims to replace and enhance existing Nushell scripts.

## Implemented Features

1.  **Global Options:**
    *   `--workdir <PATH>`: Specifies the working directory for all operations (defaults to `~/devel/connectedhomeip`). Validated to ensure `scripts/activate.sh` exists.

2.  **Building Applications (`build`)**
    *   **Usage:** `branch_diff build [OPTIONS] <APPLICATION>`
    *   **Arguments:**
        *   `APPLICATION`: Specifies the target to build (e.g., `linux-x64-all-clusters-clang`).
        *   `--tag <TAG>`: Optional tag for the output directory.
    *   **Tag Logic:** Uses `--tag` if provided, otherwise fetches the current `jj` tag from `@-` in the `workdir`.
    *   **Output Directory:** `out/branch-builds/<TAG>/` relative to `workdir`.
    *   **Execution:** Builds on host for `linux-x64-*` targets, otherwise uses a default podman container (`bld_vscode`).

3.  **Comparing Builds (`compare`)**
    *   **Usage:** `branch_diff compare [FROM_FILE] [TO_FILE] -- [EXTRA_DIFF_ARGS...]`
    *   **Arguments:**
        *   `FROM_FILE`: Optional baseline build path (e.g., `out/branch-builds/tag1/app`).
        *   `TO_FILE`: Optional comparison build path (e.g., `out/branch-builds/tag2/app`).
        *   `EXTRA_DIFF_ARGS`: Additional arguments passed to the diff script.
    *   **Interactive Mode:**
        *   Triggered if `FROM_FILE` or `TO_FILE` are omitted.
        *   Scans `out/branch-builds` for ELF files.
        *   Prompts user to select: 1. Application/File path. 2. Baseline TAG. 3. Comparison TAG (for the same application).
    *   **Comparison Logic:** Executes the diff script `scripts/tools/binary_elf_size_diff.py` using `uv run <script>`. Defaults to `--output table` if no `EXTRA_DIFF_ARGS` are provided.

## Future TODOs

### High Priority

*   **Enhanced Build Execution Control:**
    *   Add arguments to `build` to explicitly choose execution method: `--local`, `--podman`.
    *   Allow specifying podman instance name (e.g., `--podman-instance <NAME>`).
    *   Maintain current defaults if new flags are not used.
*   **Remember Last Comparison:**
    *   Store the last used `FROM_FILE` and `TO_FILE` paths (e.g., in a local config file).
    *   Add an option to `compare` (e.g., `--rerun`) to quickly re-execute the last comparison.
    *   Potentially pre-fill interactive mode defaults with last used values.
*   **Refine Interactive Application Selection:**
    *   When selecting the application in `compare` interactive mode, display the list of available tags for each application path to provide more context.

### Medium Priority

*   **Enhanced Interactive Comparison UI:**
    *   Use a TUI library (e.g., `ratatui`, `crossterm`) for a more robust interactive selection experience.
    *   Implement fuzzy searching/filtering of applications and tags.
*   **More Robust Tag Inference:**
    *   Support for git branches if a `.jj` directory is not present in `workdir` for the `build` subcommand.
    *   Allow building at the current commit hash if no tag/branch is found.

### Low Priority

*   **Configuration File:**
    *   Allow configuration of default `workdir`, podman instance, diff script path, etc., via a config file (e.g., TOML).
*   **Shell Autocomplete:**
    *   Generate shell completion scripts (Bash, Zsh, Fish, Nushell).
    *   Provide dynamic completions for tags and application names in `compare` mode.
*   **Web UI:** Eventually, a simple web interface to trigger builds and view comparison results.
*   **Database:** Store build metadata and comparison results in a simple database (e.g., SQLite) for history and analysis.
