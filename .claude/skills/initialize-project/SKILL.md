---
name: initialize-project
description: Initialize a new project from the Rust workspace template by renaming workspace members, updating Cargo metadata, and cleaning up documentation. Use when starting a new project, bootstrapping from this template, or setting up a fresh repository.
---

# Initialize Project

## Purpose

This skill bootstraps a new repository derived from the Rust workspace template. It replaces placeholder metadata, renames workspace members, and updates documentation so the new repository starts from a coherent Cargo workspace.

## Instructions

1. **Gather Information**: Ask the user for:
   - New project name (for example `acme-tools`)
   - Project description
   - Author name or author string
2. **Update Root Metadata**:
   - Update `Cargo.toml` workspace metadata such as `authors` and `repository` as appropriate.
   - Keep `[workspace.dependencies]` centralized; do not duplicate versions into member crates.
3. **Rename Workspace Members**:
   - Rename `crates/workspace-core` to `crates/<project-name>-core` unless the user wants a different library crate name.
   - Rename `crates/workspace-cli` to `crates/<project-name>-cli` unless the user wants a different binary crate name.
   - Update crate names inside each member `Cargo.toml`.
   - Update the path dependency from the CLI crate to the core crate.
4. **Update Source References**:
   - Update Rust code and docs that still mention `workspace-core`, `workspace-cli`, `{{ project_name }}`, or `{{ project_description }}`.
   - Ensure the CLI binary name matches the renamed crate when appropriate.
5. **Validate the Workspace**:
   - Run `cargo fmt --all`.
   - Run `cargo check --workspace`.
   - Run `cargo test --workspace --all-features`.
6. **Final Cleanup**:
   - Remove the initialization skill only if the user explicitly asks to strip template bootstrap helpers.

## Example

**Input**: "Initialize this project as `json-fixer`, a CLI to repair malformed JSON."

**Action**:

1. Gather the project description and author details.
2. Update `Cargo.toml`, `README.md`, and `CLAUDE.md` placeholders.
3. Rename workspace members to `json-fixer-core` and `json-fixer-cli` or user-preferred variants.
4. Update crate references and path dependencies.
5. Run Cargo validation commands.
