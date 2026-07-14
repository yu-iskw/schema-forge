---
name: pr-workflow
description: Complete pull request workflow from feature branch creation to PR submission. Use when starting a new feature, fixing a bug, or preparing code for review.
---

# Pull Request Workflow

## Purpose

A complete workflow for creating, developing, and submitting pull requests following project conventions.

## Workflow Phases

### Phase 1: Branch Setup

1. **Ensure clean state**:

   ```bash
   git status
   ```

2. **Create feature branch**:

   ```bash
   git checkout -b <type>/<description>
   ```

   Branch naming convention:
   - `feat/<description>` - New features
   - `fix/<description>` - Bug fixes
   - `docs/<description>` - Documentation
   - `refactor/<description>` - Code refactoring
   - `test/<description>` - Test additions/fixes

### Phase 2: Development

1. **Make changes** following project conventions
2. **Run quality checks**:
   ```bash
   make lint
   make test
   ```
3. **Fix any issues** using `lint-and-fix` and `test-and-fix` skills

### Phase 3: Pre-PR Verification

Before creating PR, run full verification:

1. **Invoke verifier agent**:
   Use the `verifier` subagent to run complete build → lint → test cycle

2. **Review changes**:

   ```bash
   git diff main...HEAD
   ```

3. **Invoke code-reviewer agent** (optional but recommended):
   Use the `code-reviewer` agent for quality assessment

### Phase 4: Commit Changes

1. **Stage files**:

   ```bash
   git add <specific-files>
   ```

   Avoid `git add -A` - be explicit about what's committed.

2. **Create commit**:

   ```bash
   git commit -m "type(scope): description"
   ```

   Conventional commit types:
   - `feat`: New feature
   - `fix`: Bug fix
   - `docs`: Documentation
   - `style`: Formatting (no code change)
   - `refactor`: Code restructuring
   - `test`: Test changes
   - `chore`: Maintenance tasks

### Phase 5: Create Pull Request

1. **Push branch**:

   ```bash
   git push -u origin <branch-name>
   ```

2. **Create PR using gh CLI**:

   ```bash
   gh pr create --title "type(scope): description" --body "$(cat <<'EOF'
   ## Summary
   - Brief description of changes
   - Key implementation details

   ## Changes
   - List of specific changes made

   ## Testing
   - [ ] Unit tests pass
   - [ ] Lint checks pass
   - [ ] Manual testing done (if applicable)

   ## Related Issues
   Closes #<issue-number> (if applicable)
   EOF
   )"
   ```

## PR Description Template

```markdown
## Summary

[1-3 sentences describing what this PR does and why]

## Changes

- [Specific change 1]
- [Specific change 2]
- [Specific change 3]

## Testing

- [ ] All tests pass (`make test`)
- [ ] Linting passes (`make lint`)
- [ ] Build succeeds (`make build`)
- [ ] Manual testing completed (if applicable)

## Checklist

- [ ] Code follows project style guidelines
- [ ] Self-review completed
- [ ] Documentation updated (if needed)
- [ ] No sensitive data committed
```

## Quick Commands Reference

| Task                | Command                       |
| ------------------- | ----------------------------- |
| Check branch status | `git status`                  |
| View changes        | `git diff`                    |
| Stage all changes   | `git add -A`                  |
| Stage specific file | `git add <file>`              |
| Commit changes      | `git commit -m "message"`     |
| Push branch         | `git push -u origin <branch>` |
| Create PR           | `gh pr create`                |
| View PR status      | `gh pr status`                |
| List open PRs       | `gh pr list`                  |

## Troubleshooting

### PR Checks Failing

1. Run `make lint && make test` locally
2. Fix any issues
3. Push fixes: `git add . && git commit -m "fix: address PR feedback" && git push`

### Merge Conflicts

1. Fetch latest: `git fetch origin main`
2. Rebase: `git rebase origin/main`
3. Resolve conflicts
4. Continue: `git rebase --continue`
5. Force push: `git push --force-with-lease`
