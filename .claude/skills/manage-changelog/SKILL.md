---
name: manage-changelog
description: Manage changelogs using Changie. Provides tools to initialize, add change fragments, batch releases, and merge version notes.
---

# Manage Changelog Skill

This skill enables the agent to manage project changelogs efficiently using [Changie](https://github.com/miniscruff/changie). It supports the full lifecycle from initialization to release.

## When to Use

- When you need to initialize a changelog for a new project.
- When you want to record a change (feature, bug fix, etc.) after completing a task.
- When preparing for a new release and batching unreleased changes.

## Instructions

### 1. Initialization

If the project doesn't have `changie` set up, run the initialization script:

```bash
.cursor/skills/manage-changelog/scripts/init-changie.sh
```

This will create `.changie.yaml` with standard workspace settings. For details on the configuration, see [references/configuration.md](references/configuration.md).

### 2. Adding Changes

To add a new change fragment:

```bash
changie new --kind <kind> --body "<description>"
```

- **Kinds**: See [references/configuration.md](references/configuration.md) for a list of valid kinds.
- **Helper Script**: Alternatively, use `.cursor/skills/manage-changelog/scripts/add-from-last-commit.sh` to create a fragment from the last commit message.

### 3. Batching a Release

When you are ready to create a new version:

```bash
# Preview next version
changie next auto

# Batch unreleased changes into a version
changie batch auto
```

For more details on versioning arguments and dry-run flags, see [references/command-details.md](references/command-details.md).

### 4. Merging into CHANGELOG.md

To update the main `CHANGELOG.md` file:

```bash
changie merge
```

## References

- [Best Practices](references/best-practices.md): Workflow tips and content standards.
- [Command Details](references/command-details.md): Detailed explanation of flags and utility subcommands.
- [Configuration Guide](references/configuration.md): Kinds, directory structure, and config details.
- [CLI Help Reference](references/changie-cli-help.md): Raw output from `changie --help`.
