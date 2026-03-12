# TODO

## High Priority

- **Enhanced build execution control**
  - Add `--local` / `--podman` flags to `build` to explicitly choose execution method
  - Allow specifying podman instance name via `--podman-instance <NAME>`
  - Keep current auto-detection as default when neither flag is used

- **Rerun last comparison**
  - Add `--rerun` to `compare` to re-execute the last comparison from stored defaults without any interactive prompts

## Medium Priority

- **More robust tag inference for `build`**
  - Fall back to git branch name if no `.jj` directory exists in the workdir

## Low Priority

- **Configuration file**
  - Support a config file (e.g. TOML) for more things as they are added

- **Custom tag input**
  - Implement the "Enter custom tag" option in the interactive tag selector (currently returns an error)
