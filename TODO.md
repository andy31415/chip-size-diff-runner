# TODO

## High Priority

- **Enhanced build execution control**
  - Add `--local` / `--podman` flags to `build` to explicitly choose execution method
  - Allow specifying podman instance name via `--podman-instance <NAME>`
  - Keep current auto-detection as default when neither flag is used

- **Rerun last comparison**
  - Add `--rerun` to `compare` to re-execute the last comparison from stored defaults without any interactive prompts

- **Smarter auto-tag detection**
  - If jj working copy is dirty, offer to use a fixed tag (e.g. `WIP`) or the jj short change ID — or ask the user and remember the choice for fast re-runs
  - If more than one bookmark exists at `@-`, prompt the user to select

## Medium Priority

- **More robust tag inference for `build`**
  - Fall back to git branch name if no `.jj` directory exists in the workdir
  - Allow using the current commit hash if no tag or branch is found

## Low Priority

- **Configuration file**
  - Support a config file (e.g. TOML) for default `workdir`, podman instance name, diff script path, etc.

- **Custom tag input**
  - Implement the "Enter custom tag" option in the interactive tag selector (currently returns an error)
