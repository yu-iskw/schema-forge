# Changie Configuration Reference

This project uses a standardized `changie` configuration to maintain consistency across the workspace.

## Configuration File (`.changie.yaml`)

The `.changie.yaml` file in the root directory controls how `changie` behaves. It defines:

- **Change Directory**: Where fragments and version files are stored (default: `.changes/`).
- **Kinds**: The categories of changes allowed (e.g., `feat`, `fix`).
- **Replacements**: Files to update when a new version is created (e.g., updating `package.json` version).

## Valid Change Kinds

The following kinds are configured in this project:

| Kind       | Description                              |
| :--------- | :--------------------------------------- |
| `feat`     | New features                             |
| `fix`      | Bug fixes                                |
| `docs`     | Documentation updates                    |
| `refactor` | Code refactoring (no functional changes) |
| `perf`     | Performance improvements                 |
| `test`     | Adding or updating tests                 |
| `chore`    | Maintenance, dependencies, build tasks   |

## Directory Structure

Changie organizes changes in the following hierarchy:

- `.changes/`: Root directory for all changelog data.
  - `unreleased/`: Contains individual YAML fragments for changes not yet in a version.
  - `vX.Y.Z.md`: Version-specific changelog files created during `changie batch`.
- `CHANGELOG.md`: The main, user-facing changelog file updated during `changie merge`.

## Standard Initialization

When initializing `changie` in a new package or project within this repo, the `init-changie.sh` script should be used to ensure the project matches the repository's configuration standards.
