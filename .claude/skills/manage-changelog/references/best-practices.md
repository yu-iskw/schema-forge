# Best Practices for Changelog Management

Following these best practices ensures that the project's history remains clear, automated tools work reliably, and release notes are useful for stakeholders.

## Workflow Integration

- **Atomic Changes**: Add a change fragment immediately after completing a relevant task or commit. This prevents forgetting changes and ensures each fragment is tightly scoped.
- **Task Identification**: When using `changie new`, consider if the change body should include a reference to a task ID or pull request for better traceability.
- **Kind Selection**:
  - `feat`: New features or significant enhancements.
  - `fix`: Bug fixes.
  - `docs`: Documentation-only changes.
  - `refactor`: Code changes that neither fix a bug nor add a feature.
  - `perf`: Code changes that improve performance.
  - `test`: Adding missing tests or correcting existing tests.
  - `chore`: Maintenance tasks, dependencies, or build process changes.

## Content Quality

- **Clear Descriptions**: Ensure the body of the change is descriptive enough for end-users. Avoid internal jargon or cryptic commit messages.
- **Consistency**: Maintain a consistent tense and style (e.g., imperative mood like "Add feature" rather than "Added feature") if the project follows such a convention.

## Automation and CI/CD

- **Non-interactive Mode**: When calling `changie` from scripts or CI, always provide all required flags (like `--kind` and `--body` for `new`) to avoid failures. Use `--interactive=false` to ensure it doesn't hang waiting for input.
- **CI/CD Integration**: `changie` is designed to be used in CI/CD pipelines for automated releases. Use `changie batch auto` and `changie merge` as part of your release workflow.
