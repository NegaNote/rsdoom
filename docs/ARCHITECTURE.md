  # RSDoom Architecture

  This document defines the target software architecture for a Rust port of the classic DOOM engine.

  Goals, in priority order:

  1. Safety and idiomatic Rust — no unsafe game logic, no panics on malformed data, and strong types instead of raw C-style global state and bitfields wherever it improves correctness and maintainability. Internal representation may differ from vanilla, but **simulation behavior is deterministic and reproducible across builds of the same RSDoom version**, primarily on Linux and Windows and ideally on other platforms built from the same released source.
  2. Gameplay compatibility with the WAD ecosystem — the project should load and play original IWAD/PWAD assets correctly, with a staged compatibility plan from vanilla through Boom, MBF, MBF21, and id24 features. The engine should *feel* like DOOM: correct physics, responsive input, proper enemy behavior, and accurate rendering.
  3. Demo compatibility — support playback and recording of vanilla DOOM demos and demos created by compatible source ports when all behavior-affecting factors match. These include compatibility level and flags, patches, map metadata, skill, RNG algorithm, startup state, and demo format; DSDA-Doom is a primary behavioral reference where applicable.
  4. Performance via modern architecture — a clear actor/message-passing pipeline, data-oriented structures where they help, and a renderer/audio split that does not block the simulation loop. Performance modes are available only where they preserve the determinism contract; determinism has priority over throughput.

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

  Each edge is a typed message or a publish path, not a direct function call. Actors run on dedicated OS threads where useful; input remains associated with the platform event pump when that is the native mechanism. Thread scheduling must never determine simulation results.

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

  - No panics on malformed input; invalid records become contextual, explicit errors.
  - All offsets, lengths, counts, and allocations are checked against integer overflow and configured resource limits.
  - Asset loading is bounded, cancellable, and can be structured around worker jobs.
  - The final result is an immutable, shareable `GameData` object: palettes, textures, flats, geometry, and static map metadata.

  Unsupported formats, malformed lumps, conflicting namespaces, and resource-limit violations are reported with the affected file, lump, and map. The loader does not silently substitute valid-looking defaults.

  ### 3.2 Input system

  The input system owns the platform event pump and translates low-level events into engine-level input commands such as:

  - key press/release
  - mouse motion
  - axis movement
  - quit signal
  - controller state changes

  The sim actor does not directly depend on SDL or platform-specific event types. Instead, it consumes a small, engine-defined `InputEvent` enum or equivalent command set. This keeps the simulation portable and testable.

  Core invariants:

  - Device events are translated into a bounded, explicitly timed stream of per-tic commands; simulation input is never assigned according to incidental thread timing.
  - The sim actor is the single consumer of input commands.
  - Input translation is intentionally decoupled from game logic.
  - Queue overflow, device loss, and quit events are surfaced through explicit diagnostics rather than silently ignored.

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

  #### 3.3.1 Tick protocol

  The simulation is the sole authority for the logical timeline. At each tic it:

  1. establishes the input cutoff for that tic and produces exactly one canonical command;
  2. applies queued state and control transitions at the defined tic boundary;
  3. advances the selected simulation controller in its specified object and event order;
  4. emits ordered audio/gameplay events and a new render publication;
  5. records the resulting command and, when enabled, a compact state hash for diagnostics.

  Render and audio threads may lag or skip work, but they cannot add, remove, or reorder simulation events. Replay feeds the same canonical command stream directly into this protocol.

  #### 3.3.2 Application state machine

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

  #### 3.3.3 Seamless map switching

  Map changes are not treated as an ad hoc teardown/rebuild across multiple subsystems. Instead, a map transition becomes a normal simulation transition that swaps the active gameplay state at a safe boundary.

  The immutable `GameData` may contain all levels resident in memory or provide bounded, validated level loading. In either case, switching maps then becomes:

  - finish the current simulation tick
  - apply the level transition
  - construct the new gameplay state from the selected map metadata
  - replace the old gameplay state with a fresh state in one operation
  - publish the next snapshot

  This keeps map transitions clean while preserving the idea that the renderer only sees a complete, coherent frame.

  #### 3.3.4 Menu system

  The menu logic belongs to the sim actor because it is fundamentally part of the active app state. It is modeled as a stack of screens, not a single mutable flag, so menus can nest naturally and the system can handle pause screens, option screens, and prompts in a consistent way.

  The menu is rendered as an overlay on top of the current snapshot, not as a full replacement state. This is important because the menu should not force the renderer to understand details of every simulation state; the menu is simply additional presentation data layered over whatever the current body is.

  Core rules:

  - Input goes to the menu while the menu is open.
  - Gameplay pause behavior is an explicit application-state policy, not an incidental consequence of the menu thread.
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

  Both backends consume the same snapshot structure and can be swapped at runtime without changing the sim contract. The software renderer is the accuracy reference: it should preserve classic DOOM projection, clipping, palette/colormap, visplane, sprite, fuzz, and special-effect behavior wherever the selected compatibility profile defines it. The hardware renderer may use documented approximations; those are presentation differences, not simulation differences.

  Renderer invariants:

  - It must never mutate live simulation state.
  - It must be able to render the current snapshot without any direct actor references.
  - Menu overlay is composed after the main scene, not instead of it.

  Render publications are immutable, versioned by simulation tic, and render-oriented rather than full copies of simulation state. Publication uses a bounded latest-frame policy: a slow renderer may skip intermediate frames but never creates simulation backpressure. Interpolation is disabled across map changes, teleports, and other marked state discontinuities.

  ### 3.5 Audio actor

  The audio actor owns the output device, music synthesis, SFX mixing, and event handling. It is responsible for turning gameplay events into actual audio output without forcing the simulation thread to block on audio callbacks.

  Responsibilities:

  - accept `SoundEvent` or `MusicEvent` messages from the sim actor
  - maintain a separate control-side queue or state ring
  - generate output on the relevant audio thread using a mixer callback
  - handle startup/shutdown of the sound device
  - report device loss, queue overflow, and other failures to the supervisor

  The mixer's design should keep the actual realtime callback small and predictable, with the heavier logic on the non-realtime control side. Audio is presentation, not simulation authority. Events carry their originating tic and ordering information, so delayed or unavailable audio cannot alter gameplay or replay results.

  ### 3.6 Main / supervisor

  The supervisor is intentionally minimal. It is responsible for:

  1. parsing command-line options
  2. initializing logging and error handling
  3. starting asset loading and platform subsystems
  4. creating threads and channels for sim, render, and audio
  5. running the input pump or platform event loop
  6. coordinating clean shutdown

  All heavy engine logic should live in the subsystems, not in the supervisor.

  The supervisor also owns failure propagation: actor termination, channel closure, asset-load errors, renderer failures, and audio-device failures become structured diagnostics and follow an explicit shutdown or recovery policy. No actor treats an unexpected disconnect as successful completion.

  ### 3.7 Logging, tracing, and debugging

  Diagnostics are part of the architecture rather than an afterthought. Structured logs should include subsystem, actor, map, simulation tic, profile, and relevant WAD/lump identifiers. Normal gameplay logging remains quiet; configurable levels enable detailed input, state-transition, controller, rendering, audio, and loader traces.

  Deterministic replay tooling should be able to record canonical commands, profile and asset fingerprints, per-tic state hashes, and ordered events. A desync report should identify the first divergent tic and provide enough preceding context to reproduce it. Debug builds may expose actor health, queue depth, snapshot age, and controller state without making those diagnostics part of gameplay behavior.

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
  - `Snapshot`: sim → render, published as an immutable, bounded latest-frame stream with simulation-tic identifiers
  - `SoundEvent` / `MusicEvent`: sim → audio
  - shutdown / control messages: supervisor → all actors

  This gives the engine a clear separation of concerns: simulation, presentation, and audio are independent enough to be tested and evolved separately but remain connected through a small, shared message contract.

  The message, snapshot, command, and replay contracts are versioned within the released engine format so diagnostics and regression artifacts remain interpretable.

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
  - preservation of the original C source's global mutable state patterns as a design target (internal freedom to modernize representation is required)
  - ZDoom-family map and actor semantics, including ACS and related scripting systems
  - high-performance floating-point variants until their cross-build determinism is validated

  Demo compatibility *is* a core goal (see goals section), but it is conditional: a demo is expected to match when all behavior-affecting inputs are equal. The engine is not required to reproduce undefined behavior or unsupported port semantics.

  ---

  ## 7. Demo compatibility (core goal)

  Demo compatibility is a first-class requirement achieved through deterministic simulation controllers, not a rewrite of the architecture. RSDoom's own replay determinism, vanilla demo playback, and external source-port compatibility are separate acceptance targets.

  The key constraint is that **internal representation freedom is subordinate to external determinism**. For a given released RSDoom version, controller, profile, WAD inputs, and canonical command stream, supported builds must produce the same gameplay state and event sequence, with bit-equivalent final results wherever practicable.

  ### 7.1 Determinism via simulation controller

  Each simulation controller (e.g., `VanillaFixedPoint`, `MBF21FloatPerf`) is deterministic by construction: for a fixed simulation profile, input stream, and validated game data, it always produces the same behavior. This is the foundation of replay and demo playback.

  Controllers vary in:

  - internal numeric representation (fixed-point vs floating-point)
  - explicitly specified object and event iteration order
  - physics integration method
  - ruleset semantics

  All controllers satisfy the same contract: **given identical input, profile, game data, and initial state, produce identical output**. Object order is part of the controller contract and is never unspecified for a compatibility or replay mode.

  ### 7.2 Input quantization

  The live input system may still poll devices in real time, but at each simulation tic it must convert current device state into a single canonical command object for the tic. This command is then fed into the sim actor as the authoritative input for that tick.

  This is the key boundary that makes replay/record compatibility possible without disturbing the rest of the engine.

  ### 7.3 Demo file format

  A dedicated demo module encodes:

  - header metadata and version info
  - demo format and source-port version
  - a complete simulation profile: compatibility level and flags, RNG algorithm, skill, startup state, map metadata, and applicable patches
  - WAD and patch fingerprints
  - a stream of per-tic commands

  When loading a demo, the engine validates the required inputs and instantiates the matching controller and profile before feeding the command stream into the tick protocol. RSDoom-produced replays should be bit-equivalent wherever practicable; any tolerance-based comparison is limited to explicitly presentation-only data.

  ### 7.4 External source-port compatibility

  External demo compatibility is an adapter and conformance problem, not a consequence of the controller name alone. RSDoom should use DSDA-Doom as the primary reference for supported Boom/MBF/MBF21 behavior, while preserving the original demo format and all relevant startup metadata. A demo is accepted only when its format, port behavior, compatibility flags, patches, map metadata, RNG algorithm, WAD identity, and other required factors are supported and equal. Unsupported or mismatched inputs produce an actionable incompatibility diagnostic.

  ---

  ## 8. Compatibility and performance modes

  The engine supports historical DOOM variant compatibility and, later, performance-oriented simulation. These concerns are represented separately but jointly determine behavior: a profile combines compatibility level, compatibility flags, RNG algorithm, metadata/patch inputs, and performance mode. Not every pairing is valid or demo-compatible.

  ### 8.1 Simulation controller abstraction

  Rather than scattering configuration lookups across the simulation loop, the engine uses a strategy pattern: a single `SimulationController` trait object is instantiated once per gameplay session from the selected profile. The controller owns behavior differences, while shared algorithms and data structures remain reusable. The configuration choice is paid once; it does not guarantee that every internal branch or dispatch is eliminated.

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
  - specified object and event iteration order
  - RNG behavior and algorithm, including complevel-specific seeding and sequence
  - compatibility quirks and edge cases

  The sim actor instantiates one controller at session or level setup from the complete profile. The main loop uses that controller without repeatedly looking up compatibility configuration. A map transition may replace the gameplay state, but it cannot silently change the active profile.

  ### 8.2 Demo metadata and external source-port compatibility

  Every demo identifies the profile and external inputs needed to reproduce it. A complevel/performance pair is only one part of that identity; flags, RNG algorithm, patches, UMAPINFO, skill, map, WAD fingerprints, and source-port/demo version are also relevant. Loading validates these inputs before instantiating the controller. This makes same-input replay deterministic without claiming that all demos sharing a complevel are interchangeable.

  ### 8.3 Data-model extensions

  Historical variants increase the metadata and behavior the engine can parse, but they do not change the actor topology. They fit into:

  - raw WAD parsing and conversion layers
  - ruleset-aware special interpretation within the chosen controller
  - optional metadata in the shared domain model or snapshot where the renderer needs it

  Examples:

  - generalized linedef specials
  - extended sector behavior
  - additional thing and weapon flags
  - DeHackEd/BEX behavior patches
  - UMAPINFO metadata as the priority extended map-info format, followed by other supported MAPINFO variants
  - alternate node formats normalized into a single domain representation
  - extended blockmaps, map formats, thing flags, weapons, states, and specials

  The critical design rule is that behavior variation is resolved into the profile, game data, and controller. Renderer, audio, menu, and input do not implement gameplay rules, though the render snapshot may expose profile-dependent visual data.

  ### 8.4 Implications for testing

  The strategy pattern creates clear testing boundaries:

  - **Unit tests for each controller**: Each concrete controller has its own test suite validating its specific behavior. A test for `BoomFloatPerf` exercises only that variant's code path without branches or pollution from others.
  - **Determinism validation**: Run the same command stream against independent builds of the same released version on Linux and Windows and compare per-tic state hashes and final serialized state, requiring bit equality wherever practicable.
  - **Cross-variant regression**: Play the same simple test level (e.g., E1M1 with scripted player input) under each controller and record traces. These traces will differ legitimately, but should remain stable for their respective controllers.
  - **External demo compatibility**: Collect demos from vanilla and DSDA-Doom reference configurations. Validate the complete profile and compare gameplay traces, state hashes, and completion outcomes; tolerances apply only to explicitly presentation-only data.
  - **Malformed-input coverage**: Exercise invalid WADs, patches, metadata, demos, and resource-limit cases and require contextual errors without panics.
  - **Diagnostics coverage**: Ensure replay traces, state hashes, and actor failures identify the tic, profile, map, and relevant input.

  ### 8.5 Cache efficiency

  Instantiating one controller avoids repeated configuration lookup and keeps the hot loop's compatibility decision stable. This is a clarity and predictability choice first; any dispatch or cache benefit is secondary and must not weaken deterministic ordering. Performance work is measured only after profiling and cannot change the replay contract.

  ### 8.6 Recommended sequencing

  If the project later adds compatibility work, the natural order is:

  1. implement the simulation controller abstraction with a single `VanillaFixedPoint` impl
  2. define a stable per-tic command structure and make simulation consume it
  3. verify vanilla deterministic behavior with scripted input and recorded traces
  4. add cross-build determinism checks and replay diagnostics before optimizing
  5. layer in Boom, MBF, and MBF21 fixed-point rulesets, using DSDA-Doom as the reference where applicable
  6. prioritize DeHackEd/BEX, UMAPINFO, extended map formats, flags, states, weapons, and specials needed by those profiles
  7. add performance variants only after their determinism is demonstrated across supported builds
  8. collect and test against external source-port demos for conditional cross-port compatibility
  9. treat id24 as an ongoing compatibility target rather than a prerequisite; retain ZDoom-family semantics and ACS as explicit non-goals

  ---

  ## Summary

  The architecture is intentionally conservative and future-facing:

  - one authoritative simulation actor
  - explicit state machine rather than global flags
  - immutable snapshot publication for the renderer
  - clearly bounded audio and input layers
  - a ruleset and profile compatibility layer for historical DOOM variants
  - deterministic replay, state hashing, and actionable diagnostics
  - an exactness-oriented software renderer with explicitly scoped hardware approximations

  This keeps the port faithful to the spirit of the original game while making the implementation safer, more testable, and far easier to extend by human developers over time.
