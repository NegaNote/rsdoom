  # RSDoom Architecture

  This document defines the target software architecture for a Rust port of the classic DOOM engine.

  Goals, in priority order:

  1. Safety and idiomatic Rust — no unsafe game logic, no panics on malformed data, and strong types instead of raw C-style global state and bitfields wherever it improves correctness and maintainability. Internal representation (fixed-point vs floating-point, iteration order, data layout) may differ from vanilla, but **external behavior is deterministic and reproducible**.
  2. Gameplay compatibility with WAD ecosystem — the project should load and play original IWAD/PWAD assets correctly, supporting the full range from vanilla through id24. The engine should *feel* like DOOM: correct physics, responsive input, proper enemy behavior, and accurate rendering. Rendering appearance and input responsiveness should match user expectations, but internal representation is free to evolve.
  3. Demo compatibility — support playback and recording of vanilla DOOM demos and demos created by other source ports. This is achieved through per-session simulation controller strategy that encodes a (complevel, perf_mode) pair, ensuring deterministic replay and cross-port compatibility.
  4. Performance via modern architecture — a clear actor/message-passing pipeline, data-oriented structures where they help, and a renderer/audio split that does not block the simulation loop. Performance modes (floating-point physics, modern spatial structures) are available as alternatives to fixed-point modes without sacrificing determinism.

  ---

  ## 1. High-level shape: an actor system over a shared frame bus

  Classic DOOM is a single-threaded loop built around a game tick, a renderer pass, and a presentation step. The Rust port replaces this with a small set of long-lived actors, each owning its own state exclusively and communicating via typed messages. No actor reaches into another actor's memory; this is what buys safety, clear ownership boundaries, and the ability to swap render backends without rewriting game logic.

  ```
                  ┌───────────────────────┐
                  │   Main / Supervisor   │
                  │   owns thread handles │
                  │   creates channels    │
                  └───────────┬───────────┘
                              │ spawns & wires
        ┌─────────────────────┼─────────────────────┬────────────────────┐
        ▼                     ▼                     ▼                    ▼
  ┌──────────────┐     ┌──────────────┐     ┌──────────────┐     ┌──────────────┐
  │  Input Actor │     │  Sim / Game  │     │  Renderer    │     │  Audio       │
  │  SDL event   │ ──▶ │  Actor       │ ─▶  │  Actor       │     │  Actor       │
  │  pump        │     │  owns        │     │  software or │     │  mixer +     │
  │              │     │  state       │     │  hardware    │     │  MIDI + SFX  │
  └──────────────┘     │  machine     │     │  backend     │     │  audio bus   │
                       │  + menu      │     └──────────────┘     └──────────────┘
                       └──────┬───────┘
                              │ publishes
                              ▼
                      ┌───────────────────┐
                      │ Snapshot / frame  │
                      │ immutable latest  │
                      │ state for render  │
                      └───────────────────┘
  ```

  The simulation actor owns the active application state machine and the menu stack. This is deliberate: the menu is not a separate subsystem with its own hidden state; it is part of the simulation's authority over how the game is currently running, while the renderer consumes a snapshot and composes overlays without caring why the current state changed.

  Each edge is a typed message or a publish path, not a direct function call. Actors run on dedicated OS threads where useful; input is still associated with the platform event pump when that is the native mechanism.

  ### Why actors here specifically

  | Original DOOM coupling | Problem | Actor replacement |
  |---|---|---|
  | Global input queue feeding the game loop | Shared mutable state and ordering bugs | Input actor owns SDL events and emits typed messages |
  | Renderer reading live game state while simulation mutates it | Frame tearing, inconsistent state, forced lockstep | Sim actor publishes immutable snapshots for render |
  | Audio mixed inline in the game loop | Blocking, unstable timing, mixed responsibilities | Audio actor owns the callback thread and mixer state |
  | Global `gamestate`/`menuactive` flags spread across the engine | Hard-to-reason-about transitions and menu behavior | Explicit app-state enum and menu stack owned by sim |
  | Software renderer and hardware renderer as separate code paths | Hard to swap backend safely | Renderer actor exposes one contract with multiple implementations |

  ---

  ## 2. Subsystem overview

  This architecture separates the engine into a few clear responsibilities:

  - WAD and asset loading: parse raw lump data and convert it into domain types.
  - Simulation: gameplay ticks, logic, physics, AI, specials, and state transitions.
  - Rendering: software or hardware drawing of the current snapshot with an overlay layer.
  - Audio: music synthesis, SFX mixing, and output device management.
  - Input: SDL event translation into engine-level commands.
  - Supervisor: process bootstrap, thread creation, and shutdown.

  Unlike a literal port of the C codebase, each subsystem has a stable boundary and a clear owner.

  ---

  ## 3. Subsystem detail

  ### 3.1 WAD / asset pipeline

  Responsibilities:

  - Locate and load the IWAD/PWAD set.
  - Parse the WAD directory and lump metadata.
  - Expose typed, validated views over raw lump bytes.
  - Convert raw WAD data into the engine's domain model without leaking raw on-disk representation details into gameplay code.

  The design rule is strict: parsing WAD bytes and constructing runtime game state are separate stages.

  Stage 1: raw wire-format structs

  - A loader reads the file and produces a small set of raw data structures matching the on-disk layout.
  - These structs contain no gameplay semantics and are intentionally limited to binary fields and explicit offsets.
  - Example raw types include vertices, sectors, side defs, line defs, and thing records.
  - Any parser should validate lengths, offset bounds, and null/empty lumps before returning data.

  Stage 2: conversion into domain types

  - A conversion layer maps raw records to domain representations such as `Level`, `LineDef`, `Sector`, and `Thing`.
  - This is where bitfields become typed flags, indices become validated newtypes, and special cases are interpreted according to rulesets or completeness levels.
  - The conversion layer is the only place that changes when new binary formats or WAD extensions are added.

  This separation allows the engine to evolve without rewriting raw parsing logic, while also allowing new compatibility rules to be introduced without touching renderer or simulation internals.

  Design goals for the loader:

  - No panics on malformed input; invalid records become explicit errors.
  - Asset loading is bounded and can be structured around worker jobs.
  - The final result is an immutable, shareable `GameData` object: palettes, textures, flats, geometry, and static map metadata.

  ### 3.2 Input system

  The input system owns the platform event pump and translates low-level events into engine-level input commands such as:

  - key press/release
  - mouse motion
  - axis movement
  - quit signal
  - controller state changes

  The sim actor does not directly depend on SDL or platform-specific event types. Instead, it consumes a small, engine-defined `InputEvent` enum or equivalent command set. This keeps the simulation portable and testable.

  Core invariants:

  - Input events are buffered rather than dropped under normal conditions.
  - The sim actor is the single consumer of input commands.
  - Input translation is intentionally decoupled from game logic.

  ### 3.3 Simulation actor

  The sim actor owns all mutable gameplay state:

  - players
  - mobjs
  - sectors and special geometry state
  - AI and physics state
  - collision state against the BSP tree
  - active application state
  - menu state

  This is the actor that owns the fixed-step game loop. Each tick it:

  1. drains queued input
  2. applies state transitions and menu commands
  3. advances the active application state
  4. emits sound events and music events
  5. publishes a new snapshot for render

  The simulation loop should be independent from the render frame rate. The game should run on a consistent tic cadence, while the renderer may consume a newer or interpolated frame as available.

  #### 3.3.1 Application state machine

  The simulation should not be a pile of global flags. Instead, active engine state is represented explicitly:

  ```rust
  enum AppState {
   Title(TitleState),
   Gameplay(GameplayState),
   Intermission(IntermissionState),
   EndScreen(EndScreenState),
  }
  ```

  Transitions are explicit and queued rather than spread as global actions across the codebase. They occur at logical boundaries rather than mid-update, so state changes are predictable and complete.

  This pattern makes the engine easier to reason about:

  - no dangling references to a previous level after a transition
  - no partially updated state observable between frames
  - exhaustive state handling instead of scattered `if` checks

  #### 3.3.2 Seamless map switching

  Map changes are not treated as an ad hoc teardown/rebuild across multiple subsystems. Instead, a map transition becomes a normal simulation transition that swaps the active gameplay state at a safe boundary.

  The active `GameData` may already contain all levels resident in memory. Switching maps then becomes:

  - finish the current simulation tick
  - apply the level transition
  - construct the new gameplay state from the selected map metadata
  - replace the old gameplay state with a fresh state in one operation
  - publish the next snapshot

  This keeps map transitions clean while preserving the idea that the renderer only sees a complete, coherent frame.

  #### 3.3.3 Menu system

  The menu logic belongs to the sim actor because it is fundamentally part of the active app state. It is modeled as a stack of screens, not a single mutable flag, so menus can nest naturally and the system can handle pause screens, option screens, and prompts in a consistent way.

  The menu is rendered as an overlay on top of the current snapshot, not as a full replacement state. This is important because the menu should not force the renderer to understand details of every simulation state; the menu is simply additional presentation data layered over whatever the current body is.

  Core rules:

  - Input goes to the menu while the menu is open.
  - Gameplay is paused while the menu is active, if that is the desired UX.
  - Menu actions become ordinary simulation transitions or renderer/audio commands.

  ### 3.4 Renderer actor

  The renderer actor owns the current render implementation and consumes snapshots from the sim actor. The simulation does not render directly; it publishes a state snapshot, and the renderer decides how to produce a visible frame using a backend-specific implementation.

  A typical contract looks like this:

  ```rust
  trait Renderer: Send {
   fn resize(&mut self, width: u32, height: u32);
   fn render(&mut self, snapshot: &Snapshot) -> RenderOutput;
  }
  ```

  Backends:

  - software renderer: classic column/span-style rendering, but structured safely and without shared global state
  - hardware renderer: modern GPU pipeline using textured geometry and a 3D pass, with a 2D overlay pass for UI/menu rendering

  Both backends consume the same snapshot structure and can be swapped at runtime without changing the sim contract.

  Renderer invariants:

  - It must never mutate live simulation state.
  - It must be able to render the current snapshot without any direct actor references.
  - Menu overlay is composed after the main scene, not instead of it.

  ### 3.5 Audio actor

  The audio actor owns the output device, music synthesis, SFX mixing, and event handling. It is responsible for turning gameplay events into actual audio output without forcing the simulation thread to block on audio callbacks.

  Responsibilities:

  - accept `SoundEvent` or `MusicEvent` messages from the sim actor
  - maintain a separate control-side queue or state ring
  - generate output on the relevant audio thread using a mixer callback
  - handle startup/shutdown of the sound device

  The mixer's design should keep the actual realtime callback small and predictable, with the heavier logic on the non-realtime control side.

  ### 3.6 Main / supervisor

  The supervisor is intentionally minimal. It is responsible for:

  1. parsing command-line options
  2. initializing logging and error handling
  3. starting asset loading and platform subsystems
  4. creating threads and channels for sim, render, and audio
  5. running the input pump or platform event loop
  6. coordinating clean shutdown

  All heavy engine logic should live in the subsystems, not in the supervisor.

  ---

  ## 4. Concrete threading model

  ```
  Main thread        : platform event pump + supervisor + shutdown join
  Thread "sim"       : simulation actor, fixed-timestep gameplay loop + state machine + menu state
  Thread "renderer"  : render actor, consumes snapshots and draws frames
  Thread "audio"     : audio actor control side + realtime mixer callback
  Thread pool        : transient jobs for asset parsing/conversion and optional parallel rendering work
  ```

  Channels and publication paths:

  - `InputEvent`: input → sim
  - `Snapshot`: sim → render, published as immutable data (ring buffer or atomic snapshot handle)
  - `SoundEvent` / `MusicEvent`: sim → audio
  - shutdown / control messages: supervisor → all actors

  This gives the engine a clear separation of concerns: simulation, presentation, and audio are independent enough to be tested and evolved separately but remain connected through a small, shared message contract.

  ---

  ## 5. Suggested module layout

  A representative structure could look like this:

  ```
  root/
 main.rs
 lib.rs
 wad/
   mod.rs
   file.rs
   raw.rs
   lump.rs
   text.rs
   convert.rs
 map/
   mod.rs
   bsp.rs
   geometry.rs
 sim/
   mod.rs
   state.rs
   menu.rs
   mobj.rs
   physics.rs
   specials.rs
 render/
   mod.rs
   software/
     mod.rs
     bsp_walk.rs
     columns.rs
     overlay.rs
   hardware/
     mod.rs
     pipeline.rs
     overlay.rs
 audio/
   mod.rs
   synth.rs
   mixer.rs
 input/
   mod.rs
   mapping.rs
  ```

  This is not a strict requirement; the important constraint is that module boundaries follow subsystem ownership and data flow, not the original C file structure.

  ---

  ## 6. Non-goals for the initial build

  The first implementation should intentionally avoid some of the historically complex requirements of the DOOM ecosystem:

  - frame-perfect performance-equivalent behavior (e.g., exact CPU cycle-level timing matching vanilla)
  - support for obscure or undocumented engine quirks beyond the major complevel variants
  - preservation of the original C source's global mutable state patterns as a design target (internal freedom to modernize representation is required)
  - high-performance floating-point variants (reserved for later, once determinism is validated for fixed-point modes)

  Demo compatibility *is* a core goal (see goals section), but it is achieved through deterministic simulation, not through bit-identical reproduction of undefined behavior. The engine is free to use Rust's type system, modern math, and idiomatic patterns internally, provided external behavior is reproducible and correct for each chosen controller/complevel pair.

  ---

  ## 7. Demo compatibility (core goal)

  Demo compatibility is a first-class requirement achieved through deterministic simulation controllers, not a rewrite of the architecture.

  The key insight is that **internal representation freedom and external determinism are compatible**. The engine is free to use floating-point physics, modern spatial structures, and idiomatic Rust patterns, provided that for a given simulation controller, the same input sequence always produces the same observable gameplay output.

  ### 7.1 Determinism via simulation controller

  Each simulation controller (e.g., `VanillaFixedPoint`, `MBF21FloatPerf`) is deterministic by construction: for a fixed RNG seed, input stream, and WAD, it always produces the same behavior. This is the foundation of demo playback.

  Controllers vary in:

  - internal numeric representation (fixed-point vs floating-point)
  - iteration order (deterministic vs unspecified)
  - physics integration method
  - ruleset semantics

  But all controllers satisfy the same contract: **given identical input and initial state, produce identical output**.

  ### 7.2 Input quantization

  The live input system may still poll devices in real time, but at each simulation tic it must convert current device state into a single canonical command object for the tic. This command is then fed into the sim actor as the authoritative input for that tick.

  This is the key boundary that makes replay/record compatibility possible without disturbing the rest of the engine.

  ### 7.3 Demo file format

  A dedicated demo module encodes:

  - header metadata and version info
  - simulation controller selector (complevel + perf_mode pair)
  - a stream of per-tic commands

  When loading a demo, the engine instantiates the matching controller and feeds the stored command stream into it. If implementations are correct, the replay is bit-identical (fixed-point) or numerically equivalent (floating-point).

  ### 7.4 External source-port compatibility

  By matching demo formats and controller behavior from external ports (e.g., PrBoom+, MBF21 implementations), RSDoom can play and record demos in a cross-compatible manner. The controller abstraction makes this tractable: instead of a monolithic engine with scattered compatibility checks, each controller variant is independently testable against its canonical source-port reference.

  ---

  ## 8. Compatibility and performance modes

  The engine supports both historical DOOM variant compatibility and high-performance simulation. These concerns are **orthogonal**: a given WAD may be played under any (complevel, perf_mode) pairing, and determinism is determined by both together, not one or the other.

  ### 8.1 Simulation controller abstraction

  Rather than scattering conditional branches across the simulation loop, the engine uses a strategy pattern: a single `SimulationController` trait object is instantiated at startup to encapsulate the entire (complevel, perf_mode) pair. This controller owns all behavior differences as concrete implementations with zero runtime branches on the hot path.

  Example structure:

  ```
  SimulationController (trait)
  ├─ VanillaFixedPoint
  ├─ VanillaFloatPerf
  ├─ BoomFixedPoint
  ├─ BoomFloatPerf
  ├─ MBF21FixedPoint
  ├─ MBF21FloatPerf
  └─ [Id24 variants]
  ```

  Each concrete controller encodes:

  - ruleset-specific special handling (linedef/sector/thing behavior)
  - physics integration method (fixed-point arithmetic vs floating-point)
  - object iteration order (deterministic vs unspecified)
  - RNG behavior (vanilla seeding and sequence vs modern)
  - compatibility quirks and edge cases

  The sim actor instantiates one controller at level load time based on the selected complevel and performance mode, and the main loop calls methods on that controller without branching. The choice is made once and paid for once; subsequent ticks are monomorphic.

  ### 8.2 Demo metadata and external source-port compatibility

  Every demo encodes a (complevel, perf_mode) pair in its header. This makes demos reproducible and makes it possible to play back demos recorded by other source ports with the correct behavior:

  - A Boom-era demo implicitly specifies (Boom, FixedPoint) behavior
  - An external port's high-perf demo might specify (MBF21, FloatPerf)
  - A vanilla demo specifies (Vanilla, FixedPoint)

  When loading a demo or establishing a new level under a given configuration, the engine instantiates the matching controller. This guarantees that the simulation behaves identically across replays and across ports, provided all controller implementations are correct.

  ### 8.3 Data-model extensions

  Historical variants increase the metadata and behavior the engine can parse, but they do not change the actor topology. They fit into:

  - raw WAD parsing and conversion layers
  - ruleset-aware special interpretation within the chosen controller
  - optional metadata in the shared domain model or snapshot where the renderer needs it

  Examples:

  - generalized linedef specials
  - extended sector behavior
  - additional thing and weapon flags
  - DeHackEd and MAPINFO-style metadata variants
  - alternate node formats normalized into a single domain representation

  The critical design rule: all behavior variation is encapsulated in the controller. Other subsystems (renderer, audio, menu, input) do not need to know or care which variant is active.

  ### 8.4 Implications for testing

  The strategy pattern creates clear testing boundaries:

  - **Unit tests for each controller**: Each concrete controller has its own test suite validating its specific behavior. A test for `BoomFloatPerf` exercises only that variant's code path without branches or pollution from others.
  - **Determinism validation**: Record the same input stream against two instances of the same controller and verify bit-exact match (fixed-point) or close match (floating-point, after accounting for rounding). This validates that a given controller is truly deterministic.
  - **Cross-variant regression**: Play the same simple test level (e.g., E1M1 with scripted player input) under each controller and record traces. These traces will differ legitimately, but should remain stable for their respective controllers.
  - **External demo compatibility**: Collect test demos from canonical source ports (PrBoom+, MBF21 ports, etc.). When running under the matching controller, verify that the engine produces the same output sequence or stays within acceptable tolerances (floating-point).
  - **No monomorphization fallback**: Each controller must have *at least one* test case that exercises its code path. If a controller is never tested, it is dead code.

  ### 8.5 Cache efficiency

  By instantiating a single controller and calling through its methods, the engine avoids:

  - Per-frame conditional branches on complevel or perf mode
  - Register pressure from condition codes and branch prediction state
  - Cache pollution from code paths not taken

  Modern CPU predictors handle indirect (vtable) calls efficiently after a few iterations of the main loop, so the cost is minimal once the call pattern is learned. The trade-off is that controller implementations cannot be inline-optimized as easily, but the monomorphic code within each implementation remains very optimizable.

  ### 8.6 Recommended sequencing

  If the project later adds compatibility work, the natural order is:

  1. implement the simulation controller abstraction with a single `VanillaFixedPoint` impl
  2. define a stable per-tic command structure and make simulation consume it
  3. verify vanilla deterministic behavior with scripted input and recorded traces
  4. add `VanillaFloatPerf` as the first performance variant, validating it plays the same WADs
  5. layer in Boom, MBF, and MBF21 rulesets (both fixed-point and performance variants)
  6. collect and test against external source-port demos for cross-port compatibility
  7. treat id24 as an ongoing compatibility target rather than a prerequisite

  ---

  ## Summary

  The architecture is intentionally conservative and future-facing:

  - one authoritative simulation actor
  - explicit state machine rather than global flags
  - immutable snapshot publication for the renderer
  - clearly bounded audio and input layers
  - a ruleset compatibility layer for historical DOOM variants

  This keeps the port faithful to the spirit of the original game while making the implementation safer, more testable, and far easier to extend by human developers over time.
