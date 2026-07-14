# Changie CLI Help Reference

This document contains the verbatim help output from the `changie` CLI. It serves as the source of truth for subcommands, arguments, and flags.

> **Note**: These help messages were captured from the installed version of `changie`. Re-run the help commands to refresh this document if you upgrade `changie`.

## Main Command

````texttext
Changie keeps your changes organized and attached to your code.

Changie is aimed at seamlessly integrating into your release process while also
being easy to use for developers and your release team.

Usage:
  changie [command]

Available Commands:
  batch       Batch unreleased changes into a single changelog
  completion  Generate the autocompletion script for the specified shell
  diff        diff outputs the release notes between versions.
  help        Help about any command
  init        Initialize a new changie skeleton
  latest      Echos the latest release version number
  merge       Merge all versions into one changelog
  new         Create a new change file
  next        Next echos the next version based on semantic versioning

Flags:
  -h, --help      help for changie
  -v, --version   version for changie

Use "changie [command] --help" for more information about a command.
```text

## changie new

```text
Creates a new change file.
Change files are processed when batching a new release.
Each version is merged together for the overall project changelog.

Prompts are disabled and this command will fail if any values
are not defined or valid and if any of the following are true:

Custom prompt values can also be passed in via an environment variable.
Use the following format: "${env var prefix}_CUSTOM_${custom key}=value".
Example: "CHANGIE_CUSTOM_Author=miniscruff"

1. CI env var is true
2. --interactive=false

Usage:
  changie new [flags]

Flags:
  -b, --body string        Set the change body without a prompt
  -c, --component string   Set the change component without a prompt
  -m, --custom strings     Set custom values without a prompt
  -d, --dry-run            Print new fragment instead of writing to disk
  -e, --editor             Edit body message using your text editor defined by 'EDITOR' env variable
  -h, --help               help for new
  -i, --interactive        Set missing values with prompts (default true)
  -k, --kind string        Set the change kind without a prompt
  -j, --projects strings   Set the change projects without a prompt
```text

## changie batch

```text
Merges all unreleased changes into one version changelog.

Batch takes one argument for the next version to use, below are possible options.

* A specific semantic version value, with optional prefix
* Major, minor or patch to bump one level by one
* Auto which will automatically bump based on what changes were found

The new version changelog can then be modified with extra descriptions,
context or with custom tweaks before merging into the main file.
Line breaks are added before each formatted line except the first, if you wish to
add more line breaks include them in your format configurations.

Changes are sorted in the following order:

* Components if enabled, in order specified by config.components
* Kinds if enabled, in order specified by config.kinds
* Timestamp oldest first

Usage:
  changie batch version|major|minor|patch|auto [flags]

Flags:
      --allow-no-changes     Allow batching no change fragments into an empty release note (default true)
  -d, --dry-run              Print batched changes instead of writing to disk, does not delete fragments
      --footer-path string   Path to version footer file in unreleased directory
  -f, --force                Force a new version file even if one already exists
      --header-path string   Path to version header file in unreleased directory
  -h, --help                 help for batch
  -i, --include strings      Include extra directories to search for change files, relative to change directory
  -k, --keep                 Keep change fragments instead of deleting them
  -m, --metadata strings     Metadata values to append to version
      --move-dir string      Path to move unreleased changes
  -p, --prerelease strings   Prerelease values to append to version
  -j, --project string       Specify which project version we are batching
      --remove-prereleases   Remove existing prerelease versions
```text

## changie next

```text
Next increments version based on semantic versioning.
Check latest version and increment part (major, minor, patch).
If auto is used, it will try and find the next version based on what kinds of changes are
currently unreleased.
Echo the next release version number to be used by CI tools or other commands like batch.

Usage:
  changie next major|minor|patch|auto [flags]

Flags:
  -h, --help                 help for next
  -i, --include strings      Include extra directories to search for change files, relative to change directory
  -m, --metadata strings     Metadata values to append to version
  -p, --prerelease strings   Prerelease values to append to version
  -j, --project string       Specify which project we are interested in
```text

## changie merge

```text
Merge all version files into one changelog file and run any replacement commands.

Note that a newline is added between each version file.

Usage:
  changie merge [flags]

Flags:
  -d, --dry-run                     Print merged changelog instead of writing to disk, will not run replacements
  -h, --help                        help for merge
  -u, --include-unreleased string   Include unreleased changes with this value as the header
```text

## changie init

```text
Initialize a few changie specifics.

* Folder to place all change files
* Subfolder to place all unreleased changes
* Header file to place on top of the changelog
* Output file when generating a changelog
* Unreleased folder includes a .gitkeep file

Values will also be saved in a changie config at .changie.yaml.
Default values follow keep a changelog and semver specs but are customizable.

Usage:
  changie init [flags]

Flags:
  -d, --dir string      directory for all changes (default ".changes")
  -f, --force           force initialize even if config already exist
  -h, --help            help for init
  -o, --output string   file path to output our changelog (default "CHANGELOG.md")
```text

## changie latest

```text
Echo the latest release version number to be used by CI tools.

Usage:
  changie latest [flags]

Flags:
  -h, --help               help for latest
  -j, --project string     Specify which project we are interested in
  -r, --remove-prefix      Remove 'v' prefix before echoing
      --skip-prereleases   Excludes prereleases to determine the latest version.
```text

## changie diff

```text
diff outputs to stdout the release notes between two versions.

If the argument is a number, we simply output that many previous versions.
If the argument is two versions split between triple dots similar to how GitHub
supports as a comparison then we output the release notes between those two.

Finally one last option is any of the constraints options from the semver package:
https://github.com/Masterminds/semver#checking-version-constraints.

Between versions we also add an amount of newlines specified by the AfterChangelogVersion value.

Usage:
  changie diff N|>,>=,<,<=version|versiona - versionb|start...end [flags]

Examples:
v1.20.0...v1.21.1

Flags:
  -h, --help               help for diff
  -j, --project string     Specify which project we are interested in
      --skip-prereleases   Excludes prereleases to determine the diff
```text
````
