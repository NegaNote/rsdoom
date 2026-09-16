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

DSDA-Doom is to be used as a reference for the engine's behavior when it comes to compatibility, and a copy of its code MAY be included in the folder dsda-doom (note this is gitignored intentionally). Check for the folder's existence before making any assumptions.
Use it as a reference for how the engine should behave when it comes to expected outcomes and reproducing behavior, but don't copy any of its code.

The clippy configuration is extremely pedantic, so look at the Cargo.toml before making any changes or suggestions to ensure that your code will pass clippy checks.

Rust-native iterator and functional patterns should be used wherever possible, and unsafe code should be avoided. Rust's safety guarantees are a big part of the reason this project exists in the first place, so don't go messing that up.
However, unsafe code *can* be used in specific cases where runtime verification has already made sure that invariants are met, in which case it can help performance by avoiding repeated runtime checks. As an example, if a newtype's invariant holds that the bytes it holds are already ASCII, then it's fine to use unsafe to directly convert it into &str.
Unsafe code should be used only with care, and only when it is clear that the performance benefits outweigh the potential risks. The user should be consulted before any unsafe code is added, and the user should be made aware of the risks involved.
If unsafe code is used, it should be well-documented and justified in the code comments, and Miri should be used in tests (if available -- make sure to use `+nightly` in Cargo to be able to use it if it is) to ensure that the unsafe code is sound. Agents can never add unsafe code themselves, but they can make suggestions if they can sufficiently justify their use after making sure that invariants are kept.
Naturally, unsafe code must have extensive tests to ensure that it is sound, so when evaluating potential and existing test suites that use unsafe code, make sure to consider as many edge cases as possible.

We want to enable the compiler to make optimizations like auto-vectorization wherever possible.

## Verification commands

- Formatting check: `cargo fmt --check`
- Formatting fix: `cargo fmt`
- Linting: `cargo clippy`
- Testing: `cargo test`

Code must pass all checks before being considered done. If any of the checks fail, the agent should provide guidance on how to fix the issues,
with the special exception of formatting issues, which can be fixed automatically with `cargo fmt`.

The project's clippy configuration takes an intentionally highly pedantic approach to linting, so any refactors done must be checked for clippy compliance.