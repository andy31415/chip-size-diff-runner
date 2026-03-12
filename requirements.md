# Build and Compare Tool Requirements

This document outlines the requirements for a Rust application designed to automate the building and comparison of application binaries, primarily for size difference analysis. This tool aims to replace and enhance existing Nushell scripts.

## Core Functionality

1.  **Building Applications (`build`)**
    *   The tool will support a `build` subcommand.
    *   **Required Argument:** `application` - Specifies the application/target to build (e.g., `linux-x64-all-clusters-clang`).
    *   **Optional Argument:** `--tag <TAG>` - Specifies a tag to use for the output directory.
    *   **Tag Logic:**
        *   If a `--tag` is provided, it will be used directly.
        *   If no `--tag` is provided, the tool will attempt to get the current `jj` tag from `@-`.
        *   If no `jj` tag is found at `@-`, the tool should exit with an error message.
    *   **Output Directory:** Builds will be stored in `out/branch-builds/<TAG>/`.
    *   The tool will execute the appropriate build command (e.g., `./scripts/build/build_examples.py ...`).

2.  **Comparing Builds (`compare`)**
    *   The tool will support a `compare` subcommand.
    *   **Optional Arguments:**
        *   `--from <PATH>`: Specifies the baseline binary file path.
        *   `--to <PATH>`: Specifies the comparison binary file path.
    *   **Interactive Mode:**
        *   If `--from` and `--to` are NOT provided, the tool should list all files within `out/branch-builds/` in a user-friendly way, allowing the user to select the two files to compare. The listing should clearly separate the tag and the application name.
    *   **Comparison Logic:** The tool will invoke a binary size comparison script (e.g., `~/devel/connectedhomeip/scripts/tools/binary_elf_size_diff.py`) on the two selected files.

## Future TODOS

*   **More Robust Tag Inference:**
    *   Support for git branches if a `.jj` directory is not present.
    *   Allow building at the current commit hash if no tag/branch is found.
*   **Enhanced Interactive Comparison:**
    *   Use a TUI library (e.g., `ratatui`, `crossterm`) for a better interactive selection experience when `--from` and `--to` are not provided.
    *   Implement fuzzy searching/filtering of available builds.
*   **Configuration:**
    *   Allow configuration of the build command and comparison script paths.
*   **Autocomplete:**
    *   Generate shell completion scripts (Bash, Zsh, Fish, Nushell) for subcommands and options.
    *   Provide dynamic completions for available build tags and application names in `compare` mode.
*   **Web UI:** Eventually, a simple web interface to trigger builds and view comparison results.
*   **Database:** Store build metadata and comparison results in a simple database (e.g., SQLite).
