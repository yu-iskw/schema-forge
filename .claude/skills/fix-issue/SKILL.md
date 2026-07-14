---
name: fix-issue
description: Fix a GitHub issue end-to-end. Use when given an issue number to fix, implementing bug fixes, or addressing reported problems.
---

# Fix GitHub Issue

## Purpose

A complete workflow for fixing GitHub issues from understanding the problem to creating a pull request.

## Arguments

- `$ARGUMENTS`: GitHub issue number (e.g., `123` or `#123`)

## Workflow

### 1. Understand the Issue

```bash
gh issue view $ARGUMENTS
```

Analyze:

- What is the reported problem?
- What is the expected behavior?
- What is the actual behavior?
- Are there reproduction steps?

### 2. Research the Codebase

Search for relevant code:

```bash
# Search for keywords from the issue
grep -r "keyword" src/

# Find related files
find . -name "*.py" | xargs grep -l "related_term"
```

Understand:

- Which files are involved?
- What is the current implementation?
- Where does the bug originate?

### 3. Create Feature Branch

```bash
git checkout -b fix/issue-$ARGUMENTS-<brief-description>
```

### 4. Implement the Fix

Apply the **minimum necessary change** to resolve the issue:

- Fix the root cause, not symptoms
- Don't refactor unrelated code
- Keep changes focused and reviewable

### 5. Add/Update Tests

Create tests that:

- Reproduce the original bug (should fail without fix)
- Verify the fix works (should pass with fix)
- Cover edge cases

### 6. Verify the Fix

Run the verification cycle:

```bash
make lint
make test
make build
```

Or invoke the `verifier` subagent for comprehensive validation.

### 7. Commit Changes

```bash
git add <specific-files>
git commit -m "fix(scope): brief description

Fixes #$ARGUMENTS"
```

### 8. Create Pull Request

```bash
git push -u origin fix/issue-$ARGUMENTS-<brief-description>

gh pr create --title "fix(scope): description" --body "$(cat <<'EOF'
## Summary
Fixes #$ARGUMENTS

## Problem
[Description of the issue]

## Solution
[How this PR fixes it]

## Testing
- [ ] Added test case for the bug
- [ ] All tests pass
- [ ] Manually verified the fix

## Changes
- [List specific changes]
EOF
)"
```

## Example

**Input**: `/fix-issue 42`

**Workflow**:

1. `gh issue view 42` - User reports "import fails with ModuleNotFoundError"
2. Search for import handling code
3. `git checkout -b fix/issue-42-import-error`
4. Fix the import path in `src/your_package/__init__.py`
5. Add test: `test_import_module.py`
6. `make lint && make test`
7. `git commit -m "fix(imports): resolve ModuleNotFoundError\n\nFixes #42"`
8. Create PR with reference to issue

## Quick Reference

| Task               | Command                                |
| ------------------ | -------------------------------------- |
| View issue         | `gh issue view <number>`               |
| List issues        | `gh issue list`                        |
| Create branch      | `git checkout -b fix/issue-<n>-<desc>` |
| Link PR to issue   | Include `Fixes #<n>` in commit message |
| Close issue via PR | Include `Closes #<n>` in PR body       |
