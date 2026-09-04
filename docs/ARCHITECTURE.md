  # RSDoom Architecture

  This document defines the target software architecture for a Rust port of the classic DOOM engine.

  Goals, in priority order:

  1. Safety and idiomatic Rust — no unsafe game logic, no panics on malformed data, and strong types instead of raw C-style global state and bitfields wherever it improves correctness and maintainability.
  2. Performance via modern architecture — a clear actor/message-passing pipeline, data-oriented structures where they help, and a renderer/audio split that does not block the simulation loop.
  3. Gameplay-compatible, not bit-exact — the project should behave like DOOM, load original IWAD/PWAD assets, and support modern engine ergonomics without guaranteeing exact vanilla demo compatibility or frame-perfect determinism.

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

  - exact vanilla demo playback and recording compatibility
  - frame-perfect deterministic tic execution
  - strict compatibility with every historical rule variant of the original engine
  - preserving the original C source's global mutable state patterns as a design target

  These are valid future expansions, but they should be layered in after the architecture's fundamental ownership and threading model is working.

  ---

  ## 7. Path to demo compatibility

  Demo compatibility is not a rewrite of the architecture. It is a hardening of the simulation contract.

  The main requirement is to make the game state update as a pure function of deterministic per-tic input rather than ad hoc wall-clock events. This means:

  - input is quantized to per-tic commands rather than raw asynchronous events
  - simulation logic uses a canonical order of operations per tic
  - randomness and state updates follow an explicit, reproducible sequence
  - the renderer and audio layers remain decoupled and non-deterministic by design

  The architecture already supports this direction because simulation, presentation, and audio are distinct actors and the sim actor owns the entire gameplay state.

  ### 7.1 Input quantization

  The live input system may still poll devices in real time, but at each simulation tic it must convert current device state into a single canonical command object for the tic. This command is then fed into the sim actor as the authoritative input for that tick.

  This is the key boundary that makes true replay/record compatibility possible without disturbing the rest of the engine.

  ### 7.2 Deterministic simulation core

  For compatibility-oriented work, the sim core should adopt stricter invariants:

  - fixed-point math for positions, angles, and velocities
  - canonical random number generation
  - deterministic order for object iteration and special processing
  - fixed phase order for gameplay rules and stat updates

  This is a simulation-internal concern; it does not force the renderer or audio layer to become deterministic.

  ### 7.3 Demo file format

  A dedicated demo module should encode:

  - header metadata and version info
  - selected ruleset or complevel
  - a stream of per-tic commands

  This can be layered onto the engine once the sim is already consuming a stable command format.

  ---

  ## 8. Path to Boom / MBF / MBF21 / id24 support

  Historical source-port feature sets should be treated as a compatibility layer over a stable engine core, not as a rewrite of the architecture.

  ### 8.1 A compatibility layer (`sim::compat`)

  Introduce a ruleset abstraction that selects behavior based on the target compatibility level. This could be modeled as:

  ```rust
  enum Complevel {
   Vanilla,
   Boom,
   Mbf,
   Mbf21,
   Id24,
  }
  ```

  Then the engine can carry a per-run or per-demo compatibility configuration through the simulation subsystem. This is preferable to scattering `#[cfg]` branches across the codebase.

  The compatibility layer should control:

  - special interpretation rules
  - monster behavior changes
  - gameplay quirks and edge-case semantics
  - optional metadata interpretation from lumps or text files

  ### 8.2 Data-model extensions

  Historical variants increase the amount of metadata and behavior the engine can parse, but they do not need to change the actor topology. They fit naturally into:

  - raw WAD parsing and conversion layers
  - ruleset-aware special interpretation in the sim actor
  - optional metadata carried in the shared domain model or snapshot where the renderer needs it

  Examples:

  - generalized linedef specials
  - extended sector behavior
  - additional thing and weapon flags
  - DeHackEd and MAPINFO-style metadata variants
  - alternate node formats normalized into a single domain representation

  The important part is that these stay in the loader/ruleset code paths rather than becoming new fundamental subsystems.

  ### 8.3 Recommended sequencing

  If the project later adds compatibility work, the natural order is:

  1. define a stable per-tic command structure and make simulation consume it
  2. migrate the sim core to fixed-point math and canonical RNG behavior
  3. verify vanilla deterministic behavior first
  4. layer in Boom, MBF, and MBF21 rulesets incrementally
  5. treat id24 as an ongoing compatibility target rather than a prerequisite for the core architecture

  ---

  ## Summary

  The architecture is intentionally conservative and future-facing:

  - one authoritative simulation actor
  - explicit state machine rather than global flags
  - immutable snapshot publication for the renderer
  - clearly bounded audio and input layers
  - a ruleset compatibility layer for historical DOOM variants

  This keeps the port faithful to the spirit of the original game while making the implementation safer, more testable, and far easier to extend by human developers over time.
