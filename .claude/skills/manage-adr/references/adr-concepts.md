# Architecture Decision Records (ADRs)

An Architecture Decision Record (ADR) is a short text file that captures an important architectural decision made along with its context and consequences.

## Why use ADRs?

- **Historical Context**: Understand _why_ a decision was made months or years later.
- **Onboarding**: Help new team members understand the architectural evolution.
- **Alignment**: Ensure everyone is on the same page regarding technical choices.
- **Avoiding Regressions**: Prevent re-litigating old decisions without new context.

## ADR Structure (Nygard Format)

1. **Title**: Number and short noun phrase (e.g., "1. Record architecture decisions").
2. **Status**: Proposed, Accepted, Superseded, etc.
3. **Context**: The situation and the problem being solved.
4. **Decision**: The chosen solution.
5. **Consequences**: The results of the decision (good and bad).

## Managing Lifecycle

- **Superseding**: When a new decision replaces an old one, the old one is marked as "Superseded" and links to the new one.
- **Linking**: Related decisions should be linked (e.g., "Amends", "Depends on").

## Tools

We use `adr-tools` to manage these records. It handles numbering, linking, and status updates automatically.
