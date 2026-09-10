# AGENTS.md

## Agent scope

Agents are for **code review** and **debugging** only.
They must not directly implement code changes themselves -- the user is to be trusted to make their own code changes,
and the agent is to provide guidance and suggestions only. However, for *extremely simple* code changes, the agent may
ask the user to implement them directly, if the user is comfortable with that, or even implement them directly
after *explicitly* asking the user for permission. Code changes can never be made without the user's explicit permission.

Documentation, code review, and debugging are the primary responsibilities of agents. Documentation may also be edited by agents,
but changes must be reviewed by the user before being committed.

Agents must not perform source control operations under any circumstances, and must not make any assumptions about the user's source control workflow.

## Project expectations

This repository is a safe, idiomatic Rust port of the 90's DOOM engine.
Architecture decisions live in `docs/ARCHITECTURE.md` and should be treated as the source of truth.
Freedoom assets are included, but the engine should be able to run with any WAD files.

## Verification commands

- Formatting check: `cargo fmt --check`
- Formatting fix: `cargo fmt`
- Linting: `cargo clippy`
- Testing: `cargo test`

Code must pass all checks before being considered done. If any of the checks fail, the agent should provide guidance on how to fix the issues,
with the special exception of formatting issues, which can be fixed automatically with `cargo fmt`.

The project's clippy configuration takes an intentionally highly pedantic approach to linting, so any refactors done must be checked for clippy compliance.