# Changie Command Details

This document provides deeper technical details on specific `changie` commands and flags commonly used in this project.

## Core Command Details

### Adding Changes (`changie new`)

- **Non-interactive Mode**: In CI or non-interactive environments (when `--interactive=false` or `CI` env var is true), both `--kind` (`-k`) and `--body` (`-b`) are **required**.
- **Dry Run**: Use the `--dry-run` (`-d`) flag to preview the fragment in stdout without writing a file to `.changes/unreleased/`.

### Batching Releases (`changie batch`)

- **Version Selection**:
  - `auto`: Automatically determine version bump based on kinds found in unreleased fragments.
  - `major` | `minor` | `patch`: Explicitly bump the corresponding semver part.
  - `<version>`: Specify an exact version string (e.g., `v1.2.3`).
- **Dry Run**: Use `--dry-run` to see which fragments will be moved to the new version directory and what the resulting version file will look like.

### Merging (`changie merge`)

- **Include Unreleased**: Use `--include-unreleased "Unreleased"` to generate a preview of the changelog including pending changes.

## Utility Commands

These commands are useful for automation scripts and inspection.

### `changie latest`

Echoes the latest released version number. Useful for determining the starting point for a new release.

```bash
changie latest
```

### `changie next`

Echoes the next version based on semantic versioning rules and unreleased fragments.

```bash
changie next auto
```

### `changie diff`

Outputs the release notes between two versions.

```bash
changie diff <version1>...<version2>
```

## Reference

For a full list of commands and flags, refer to [changie-cli-help.md](changie-cli-help.md).
