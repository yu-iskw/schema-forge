---
name: improve-claude-config
description: Self-improvement skill for evolving Claude Code configuration. Use when you notice repeated mistakes, want to add new workflows, or optimize the development experience. Enables Claude to improve its own CLAUDE.md, skills, hooks, and agents.
---

# Improve Claude Configuration

## Purpose

This skill enables Claude Code to evolve and improve its own configuration based on observed patterns, user feedback, and development needs. It implements the self-improvement paradigm for AI-assisted development.

## When to Use

Invoke this skill when:

- You've explained the same concept to Claude multiple times
- Claude repeatedly makes the same type of mistake
- A new workflow pattern has emerged that should be automated
- Hooks could prevent recurring issues
- New skills would benefit the project

## Self-Improvement Workflow

### 1. Analyze Current State

Read and understand the current configuration:

```bash
# View CLAUDE.md
cat CLAUDE.md

# List all skills
ls -la .claude/skills/

# List all agents
ls -la .claude/agents/

# View current hooks
cat .claude/settings.json | jq '.hooks'
```

### 2. Identify Improvement Opportunity

Based on user input or observed patterns, determine what type of improvement is needed:

| Pattern                     | Action                             |
| --------------------------- | ---------------------------------- |
| Repeated explanation        | Add to CLAUDE.md or create a skill |
| Recurring mistake           | Add rule to CLAUDE.md              |
| Manual repetitive task      | Create a hook                      |
| Complex workflow            | Create a skill                     |
| Specialized task delegation | Create an agent                    |

### 3. Implement Improvement

#### Adding Rules to CLAUDE.md

For project conventions or gotchas Claude keeps forgetting:

```markdown
## Recent Learnings (append to CLAUDE.md)

- [Date]: Description of rule and why it matters
```

Keep CLAUDE.md under 150 lines. Move detailed content to skills.

#### Creating a New Skill

1. Create directory: `.claude/skills/<skill-name>/`
2. Create `SKILL.md` with:

```yaml
---
name: skill-name
description: Clear description of when to use this skill
---

# Skill Title

## Purpose
What this skill does and why

## Instructions
Step-by-step workflow

## Examples
Concrete usage examples
```

#### Creating a New Hook

Add to `.claude/settings.json`:

```json
{
  "hooks": {
    "PostToolUse": [
      {
        "matcher": "ToolName",
        "hooks": [
          {
            "type": "command",
            "command": "script.sh"
          }
        ]
      }
    ]
  }
}
```

#### Creating a New Agent

Create `.claude/agents/<agent-name>.md`:

```yaml
---
name: agent-name
description: What this agent specializes in
tools: Read, Grep, Glob
model: sonnet
---
# Agent Role Description

Instructions for the agent...
```

### 4. Validate Changes

After making changes:

1. Verify JSON syntax: `cat .claude/settings.json | jq .`
2. Test hooks if added
3. Verify skill loads correctly

### 5. Document the Change

Add a comment in the relevant file explaining:

- What was changed
- Why it was changed
- What problem it solves

## Examples

### Example 1: Adding a Common Gotcha

**Trigger**: User says "Claude keeps forgetting to use `uv run` prefix"

**Action**: Add to CLAUDE.md:

```markdown
## Common Gotchas

- Always use `uv run` to execute Python commands in the virtual environment
```

### Example 2: Creating a Skill for PR Descriptions

**Trigger**: User repeatedly explains how to write PR descriptions

**Action**: Create `.claude/skills/write-pr-description/SKILL.md` with the standard format

### Example 3: Adding a Safety Hook

**Trigger**: Claude accidentally ran a command that should be blocked

**Action**: Add to `.claude/hooks/block-dangerous.sh` and update settings.json

## Best Practices

1. **Be minimal**: Only add what's necessary
2. **Be specific**: Vague rules are ignored
3. **Test changes**: Validate hooks and skills work
4. **Version control**: Commit configuration changes with clear messages
5. **Review periodically**: Remove outdated rules
6. **Document why**: Future Claude sessions need context

## Self-Evolution Metrics

Track improvement effectiveness:

- Fewer repeated explanations needed
- Fewer manual corrections required
- More autonomous task completion
- Reduced context switching
