# Nick's Skills

A growing collection of reusable agent skills written and shared by Nick.

Each skill lives in its own folder with a `SKILL.md` entry point and any supporting resources it needs.

## Available skills

| Skill | What it does |
| --- | --- |
| [Master Controller](skills/master-controller/SKILL.md) | Runs PowerShell and CMD scripts through a paired worker runner, and provides the runner for deployment. |
| [Toula the Fixer](skills/toula-the-fixer/SKILL.md) | Diagnoses and resolves IT incidents across Windows, macOS, and Linux, either through verified direct access or by guiding an on-site engineer. |
| [Unslop](skills/unslop/SKILL.md) | Removes common AI writing patterns from user-facing prose while preserving requested style and technical accuracy. |

### Master Controller

Master Controller submits `.ps1`, `.cmd`, and `.bat` files through a worker-runner directory and returns the completed textual output. The skill bundles the controller and the paired worker runner. The controller supports Windows PowerShell 5.1 and PowerShell 7; the runner supports Windows PowerShell 1.0 and later.

### Toula the Fixer

Toula works from evidence, verifies the affected host, and uses small troubleshooting steps. It keeps a full incident State, tracks changes and cleanup, and provides short or detailed handoff reports. Complex plans can be approved by their plan ID.

The skill contains instructions and reference documents; it does not include an executable troubleshooting program. Any direct actions depend on the assistant's available tools, verified access, and your authorization.

### Unslop

Unslop edits user-facing prose to remove filler, vague claims, canned phrasing, and other common AI writing patterns. Its rules are strong defaults, with exceptions for accuracy, quotations, code, required formats, and an explicitly requested voice.

## Install in Codex

Ask the built-in skill installer for the skill you want:

```text
$skill-installer Install the skill at https://github.com/ndemou/Nicks-Skills/tree/main/skills/master-controller
$skill-installer Install the skill at https://github.com/ndemou/Nicks-Skills/tree/main/skills/toula-the-fixer
$skill-installer Install the skill at https://github.com/ndemou/Nicks-Skills/tree/main/skills/unslop
```

Alternatively, download or clone this repository and copy the entire `skills/<skill-name>` folder into your personal skills directory:

- macOS/Linux: `~/.agents/skills/`
- Windows: `%USERPROFILE%\.agents\skills\`

Keep any `scripts/`, `references/`, and `agents/` subfolders with `SKILL.md`. Codex detects newly installed skills automatically; restart it if the skill does not appear.

Then invoke the installed skill, for example:

```text
$master-controller Run C:\Work\diagnose.ps1 through the worker runner at C:\it\temp and interpret the result.
$toula-the-fixer Help diagnose why users cannot reach our file server. Ask me for any missing context and start with read-only checks.
```

## Repository layout

```text
skills/
  master-controller/
    SKILL.md
    agents/
      openai.yaml
    scripts/
      Master-Controller.ps1
      Worker-Runner.ps1
  toula-the-fixer/
    SKILL.md
    agents/
      openai.yaml
    references/
      action-mode.md
      command-delivery.md
      status-reports.md
  unslop/
    SKILL.md
```

## Adding future skills

Create a new folder under `skills/`, include a `SKILL.md` with a unique `name` and a focused `description`, and add the skill to the table above. Keep supporting files inside that skill's folder and link to them using relative paths.

This repository follows the [official skill guidance](https://learn.chatgpt.com/docs/build-skills). It currently shares standalone skill folders; that guidance recommends plugin packaging for installable distribution across ChatGPT and Codex.
