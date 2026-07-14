---
name: setup-dev-env
description: Set up the development environment for the project. Use when starting work on the project, when dependencies are out of sync, or to fix environment setup failures.
---

# Setup Development Environment

This skill automates the process of setting up the development environment to ensure all tools and dependencies are correctly installed and configured.

## Workflow Checklist

- [ ] **Step 1: Environment Validation**
  - [ ] Check Python version against `.python-version`
  - [ ] Check for `trunk` installation
  - [ ] Check for `uv` installation
- [ ] **Step 2: Dependency Installation**
  - [ ] Run `make setup`
- [ ] **Step 3: Tooling Setup**
  - [ ] Run `trunk install` to fetch managed linters and formatters

## Detailed Instructions

### 1. Environment Validation

#### Python Version

Read the `.python-version` file in the workspace root. Ensure the current Python environment matches this version. If there's a mismatch, inform the user to switch Python versions (e.g., using `pyenv` or `asdf`).

#### Tooling Installation

Check if `trunk` and `uv` are installed.
If not found, advise the user to install them. On macOS, use:

```bash
brew install trunk-io uv
```

### 2. Dependency Installation

Run the following command at the workspace root to install all project dependencies. Refer to [../common-references/python-commands.md](../common-references/python-commands.md) for more commands.

```bash
make setup
```

This command runs `dev/setup.sh`, which installs `uv` if needed (via pip), creates a virtual environment, and syncs dependencies.

### 3. Tooling Setup

Trunk manages linters and formatters hermetically. Run the following command to ensure all required tools are downloaded and ready.

```bash
trunk install
```

## Success Criteria

- All Python dependencies are installed successfully in the virtual environment.
- `trunk` and `uv` are installed.
- The Python version matches the requirement in `.python-version`.

## Post-Setup Verification

To ensure the environment is fully operational:

1. **Invoke Verifier**: Run the `verifier` subagent ([../../agents/verifier.md](../../agents/verifier.md)). This confirms that the freshly installed dependencies allow for a successful build, pass lint checks, and satisfy all unit tests.
2. **Handle Failure**: If the `verifier` fails, follow its reporting to resolve environment-specific issues.
