  # RSDoom Architecture

  This document defines the target software architecture for a Rust port of the classic DOOM engine.

  Goals, in priority order:

  1. Safety and idiomatic Rust — no unsafe game logic, no panics on malformed data, and strong types instead of raw C-style global state and bitfields wherever it improves correctness and maintainability. Internal representation may differ from vanilla, but simulation behavior is deterministic and reproducible across builds of the same RSDoom version, primarily on Linux and Windows and ideally on other platforms built from the same released source.
  2. Gameplay compatibility with the WAD ecosystem — the project should load and play original IWAD/PWAD assets correctly, with a staged compatibility plan from vanilla through Boom, MBF, MBF21, and id24 features. The engine should feel like DOOM: correct physics, responsive input, proper enemy behavior, and accurate rendering.
  3. Demo compatibility — support playback and recording of vanilla DOOM demos and demos created by compatible source ports when all behavior-affecting factors match. These include compatibility level and flags, patches, map metadata, skill, RNG algorithm, startup state, and demo format; DSDA-Doom is a primary behavioral reference where applicable.
  4. Performance via deliberate architecture — a deterministic, single-threaded actor model, disciplined ownership boundaries, and a render/audio split that does not compromise simulation correctness. Performance is an implementation detail, not a second orthogonal gameplay mode; determinism has priority over throughput, and compatibility rulesets remain the authority over behavior.

  ---

  ## 1. High-level shape: a single-threaded actor system over a shared frame bus

  Classic DOOM is a single-threaded loop built around a game tick, a renderer pass, and a presentation step. The Rust port keeps that model as the authoritative execution architecture, but reinterprets it in actor terms: a small set of long-lived actors exist as conceptual subsystems with explicit state ownership and typed message contracts, yet they all run on the same deterministic engine thread.

  This is not a "fake actor system" that merely names modules; it is a deliberate single-threaded actor pattern: actors own state, communicate via messages, and schedule work in a fixed order. The critical difference from a multithreaded actor framework is that no actor can race against another actor because the engine loop is the sole place where simulation state is mutated.

  ```
                        ┌────────────────────────────┐
                        │      Supervisor / Loop     │
                        │   owns app lifetime,      │
                        │   event pump, tick order   │
                        └──────────────┬─────────────┘
                                       │
              ┌────────────────────────┼────────────────────────┐
              ▼                        ▼                        ▼
    ┌──────────────────┐      ┌──────────────────┐      ┌──────────────────┐
    │ Input Actor      │      │ Sim / Game Actor │      │ Renderer Actor   │
    │ event pump +     │ ──▶  │ owns mutable     │ ──▶  │ consumes snapshot│
    │ command queue    │      │ gameplay state   │      │ and draws frame  │
    └──────────────────┘      │ + menu + app     │      └──────────────────┘
                              │ state machine    │
                              └────────┬─────────┘
                                       │ publishes
                                       ▼
                              ┌──────────────────┐
                              │ Snapshot / Frame │
                              │ immutable render │
                              │ data for the     │
                              │ current tic      │
                              └──────────────────┘

    Audio is not a second thread in the gameplay core: it is a subsystem driven by commands and a mixer state owned by the same deterministic event loop. Optional background jobs may exist for asset loading or decompression, but they never mutate active gameplay state.
  ```

  The simulation actor owns the active application state machine and the menu stack. This is deliberate: the menu is not a separate subsystem with its own hidden state; it is part of the simulation's authority over how the game is currently running, while the renderer consumes snapshots and composes overlays without caring why the current state changed.

  Each edge is a typed message or a publishing path, not a direct function call. Scheduling is determined by a single engine loop, and that loop is the source of truth for tick order. We do not accept thread scheduling as a gameplay input.

  ### Why actors here specifically

  | Original DOOM coupling | Problem | Single-threaded actor replacement |
  |---|---|---|
  | Global input queue feeding the game loop | Shared mutable state and ordering bugs | Input actor owns platform events and emits typed commands to the sim loop |
  | Renderer reading live game state while simulation mutates it | Frame tearing, inconsistent state, forced lockstep | Sim actor publishes immutable snapshots for render |
  | Audio mixed inline in the game loop | Blocking, unstable timing, mixed responsibilities | Audio subsystem owns mixer state and consumes commands in deterministic tick order |
  | Global `gamestate`/`menuactive` flags spread across the engine | Hard-to-reason-about transitions and menu behavior | Explicit app-state enum and menu stack owned by sim |
  | Software renderer and hardware renderer as separate code paths | Hard to swap backend safely | Renderer actor exposes one contract with multiple implementations |

  The design still uses the actor model's advantages — ownership boundaries, message passing, and replacement of subsystems by interfaces — but without multithreaded complexity. The engine remains deterministic because there is only one mutation authority: the simulation tick.

  ---

  ## 2. Subsystem overview

  This architecture separates the engine into a few clear responsibilities:

  - WAD and asset loading: parse raw lump data and convert it into domain types.
  - Simulation: gameplay ticks, logic, physics, AI, specials, and state transitions.
  - Rendering: software or hardware drawing of the current snapshot with an overlay layer.
  - Audio: music synthesis, SFX mixing, and output device management.
  - Input: SDL event translation into engine-level commands.
  - Supervisor: process bootstrap, loop scheduling, and shutdown.

  Unlike a literal port of the C codebase, each subsystem has a stable boundary and a clear owner. The key difference from a multithreaded architecture is that those owners are logical actors, not OS threads.

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

  - Device events are translated into a bounded, explicitly timed stream of per-tic commands; simulation input is never assigned according to incidental OS scheduling.
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

  Render and audio work may lag or skip work, but they cannot add, remove, or reorder simulation events. Replay feeds the same canonical command stream directly into this protocol.

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
  - Gameplay pause behavior is an explicit application-state policy, not an incidental consequence of an unrelated scheduling layer.
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

  The audio actor owns the output device, music synthesis, SFX mixing, and event handling. It is responsible for turning gameplay events into actual audio output without forcing the simulation loop to block on audio callbacks.

  Responsibilities:

  - accept `SoundEvent` or `MusicEvent` messages from the sim actor
  - maintain a separate control-side queue or state ring
  - generate output on the relevant audio callback thread or mixer loop without mutating gameplay state
  - handle startup/shutdown of the sound device
  - report device loss, queue overflow, and other failures to the supervisor

  The mixer's design should keep the realtime callback small and predictable, with the heavier logic on the non-realtime control side. Audio is presentation, not simulation authority. Events carry their originating tic and ordering information, so delayed or unavailable audio cannot alter gameplay or replay results.

  In the single-threaded variant, audio is still a subsystem with a distinct owner and message contract; it simply does not get its own simulation thread. It consumes commands from the engine loop and runs in the same process timeline, preserving determinism while still separating concerns.

  ### 3.6 Main / supervisor

  The supervisor is intentionally minimal. It is responsible for:

  1. parsing command-line options
  2. initializing logging and error handling
  3. starting asset loading and platform subsystems
  4. creating the engine loop and scheduling actors
  5. running the platform event pump and dispatching input
  6. coordinating clean shutdown

  All heavy engine logic should live in the subsystems, not in the supervisor.

  The supervisor also owns failure propagation: actor termination, channel closure, asset-load errors, renderer failures, and audio-device failures become structured diagnostics and follow an explicit shutdown or recovery policy. No actor treats an unexpected disconnect as successful completion.

  ### 3.7 Logging, tracing, and debugging

  Diagnostics are part of the architecture rather than an afterthought. Structured logs should include subsystem, actor, map, simulation tic, profile, and relevant WAD/lump identifiers. Normal gameplay logging remains quiet; configurable levels enable detailed input, state-transition, controller, rendering, audio, and loader traces.

  Deterministic replay tooling should be able to record canonical commands, profile and asset fingerprints, per-tic state hashes, and ordered events. A desync report should identify the first divergent tic and provide enough preceding context to reproduce it. Debug builds may expose actor health, queue depth, snapshot age, and controller state without making those diagnostics part of gameplay behavior.

  ---

  ## 4. Concrete execution model

  The engine intentionally runs on a single deterministic scheduler:

  ```
  Main thread / engine loop:
    - platform event pump
    - input queue draining
    - sim tick execution
    - render snapshot publication
    - audio command dispatch
    - shutdown and diagnostics

  Optional background jobs:
    - asset parsing/conversion
    - file decompression or cache warming
    - any non-simulation work that can be cancelled cleanly

  These jobs must never mutate live gameplay state directly. They publish immutable results back to the engine loop.
  ```

  The loop order is fixed and semantically meaningful. A typical frame is:

  1. poll platform events and enqueue `InputEvent`s
  2. run the simulation actor for the next logical tick
  3. update audio subsystem state from generated events
  4. publish the latest snapshot for the renderer
  5. render the current snapshot to the window or backbuffer
  6. advance diagnostics, logging, and replay capture

  This gives the engine a clear separation of concerns: simulation, presentation, and audio are independent enough to be tested and evolved separately but remain connected through a small, shared message contract. Importantly, the game loop is authoritative. We do not ask the OS scheduler to resolve gameplay ordering.

  The message, snapshot, command, and replay contracts are versioned within the released engine format so diagnostics and regression artifacts remain interpretable.

  ---

  ## 5. Suggested module layout

  A representative structure for a project of this type could look like this:

  ```
  root/
   Cargo.toml
   src/
     main.rs
     lib.rs
     argparse.rs
     config/
       mod.rs
       profile.rs
     wad/
       mod.rs
       builder.rs
       raw/
         mod.rs
         tests.rs
       assets/
         mod.rs
         gfx.rs
       convert.rs
     sim/
       mod.rs
       state.rs
       world.rs
       compat/
         mod.rs
         profile.rs
         options.rs
     render/
       mod.rs
       snapshot.rs
       software/
         mod.rs
     audio/
       mod.rs
       mixer.rs
     input/
       mod.rs
       mapping.rs
  ```

  This is not a strict requirement; the important constraint is that module boundaries follow subsystem ownership and data flow, not the original C file structure. The project should continue to grow in a direction where core entrypoints and CLI wiring sit near the root, WAD parsing and asset conversion remain a foundational subsystem, and simulation, rendering, audio, and input are introduced as the engine acquires gameplay and presentation responsibilities.

  ---

  ## 6. Non-goals for the initial build

  The first implementation should intentionally avoid some of the historically complex requirements of the DOOM ecosystem:

  - frame-perfect performance-equivalent behavior (e.g., exact CPU cycle-level timing matching vanilla)
  - preservation of the original C source's global mutable state patterns as a design target (internal freedom to modernize representation is required)
  - ZDoom-family map and actor semantics, including ACS and related scripting systems
  - high-performance floating-point variants until their cross-build determinism is validated
  - networked multiplayer, which would require significant changes and additional design work to the simulation architecture and tick protocol

  Demo compatibility is a core goal (see goals section), but it is conditional: a demo is expected to match when all behavior-affecting inputs are equal. The engine is not required to reproduce undefined behavior or unsupported port semantics.

  ---

  ## 7. Demo compatibility (core goal)

  Demo compatibility is a first-class requirement achieved through deterministic simulation controllers, not by adding hidden concurrency or implicit timing. RSDoom's own replay determinism, vanilla demo playback, and external source-port compatibility are separate acceptance targets.

  The key constraint is that internal representation freedom is subordinate to external determinism. For a given released RSDoom version, controller, profile, WAD inputs, and canonical command stream, supported builds must produce the same gameplay state and event sequence, with bit-equivalent final results wherever practicable.

  ### 7.1 Determinism via a single simulation controller and profile

  The engine uses a single simulation controller, but it is configured by the active `CompatibilityProfile` and decoded options data for the session. For a fixed profile, input stream, and validated game data, it always produces the same behavior. This is the foundation of replay and demo playback.

  The profile and option payload vary in:

  - compatibility level and historical ruleset semantics
  - internal numeric representation when that representation is intentionally selected for a profile (fixed-point vs floating-point)
  - explicitly specified object and event iteration order
  - physics integration method
  - compatibility flags, metadata overrides, and feature toggles

  The engine's single controller satisfies one contract: given identical input, profile, game data, and initial state, it produces identical output. Object order is part of the profile contract and is never unspecified for a compatibility or replay mode.

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

  ## 8. Compatibility modes

  The engine supports historical DOOM variant compatibility and RSDoom's own compatibility mode. These are not separate orthogonal settings; they are the rulesets that determine gameplay behavior. Performance remains an implementation concern, not a second gameplay mode. A profile combines compatibility level, compatibility flags, RNG algorithm, metadata/patch inputs, and the active compatibility mode. Not every pairing is valid or demo-compatible.

  RSDoom's compatibility mode is a first-class engine ruleset. It may define its own features, gameplay decisions, and compatibility deviations when those choices are deliberate and documented. Unlike a performance toggle, it is allowed to change simulation behavior when that is the intended contract of the chosen RSDoom mode.

  To keep the engine easier to reason about, maintain, and test, we favor a single simulation controller and explicit runtime configuration over polymorphic per-ruleset controller objects. The controller is not a trait with many implementations. Instead, there is one canonical gameplay engine configured with a `CompatibilityProfile` and an already-decoded `OptionsLump` (or equivalent structured options data). The compatibility level and option payload decide behavior at runtime, while the rest of the engine sees a single, normalized execution path.

  This design sacrifices some theoretical runtime dispatch efficiency, but it wins substantial maintenance and testability benefits:

  - one code path for simulation logic, with behavior selected by data rather than type hierarchy
  - easier auditing of compatibility differences against a single profile object
  - simpler unit and regression tests that can create a profile and replay the same commands
  - lower risk of divergent controller implementations drifting out of sync
  - clear inheritance semantics for mode behavior without requiring trait object sprawl

  ### 8.1 Compatibility profile and option model

  The core configuration object is a normalized `CompatibilityProfile`:

  ```rust
  struct CompatibilityProfile {
      level: CompatibilityLevel,
      options: OptionsSet,
      rng: RngAlgorithm,
      skill: Skill,
      map: MapIdentity,
      wad_fingerprints: WadFingerprints,
      demo_metadata: Option<DemoMetadata>,
  }
  ```

  `CompatibilityLevel` encodes the historical DOOM family level (vanilla, Boom, MBF, MBF21, id24, etc.), while `OptionsSet` is the decoded result of the engine options lump or equivalent metadata payload. That payload may include compatibility flags, custom rules, feature toggles, and per-ruleset behavior selectors. The important rule is that the profile is data, not a type-level declaration.

  This means the engine still has one simulation controller, but it receives a profile that tells it how to interpret the world. A compat mode is therefore not a different execution engine. It is a semantic overlay on the same simulation engine with a precise set of inherited behavior and additions.

  ### 8.2 Mode inheritance model

  The compatibility modes form a hierarchy, not a fragmented trait set:

  ```
  Vanilla
    └─ Boom
        └─ MBF
            └─ MBF21
                └─ RSDoom
                    └─ RSDoom+SpecializedExtensions
  ```

  In practice, the inherited semantics are encoded in the profile and option decoder, not in separate `trait` implementations. The profile should be able to answer questions like:

  - does this mode allow linedef special variants X/Y/Z?
  - does it use fixed-point or float-based physics?
  - which RNG algorithm and seeding rules apply?
  - which map metadata extensions are considered valid?
  - which features are inherited from lower compatibility levels and which are added by RSDoom?

  This gives RSDoom a clear semantic identity: it inherits the lower compatibility behaviors it chooses to preserve, and adds selected engine-native rules and gameplay features on top. Any RSDoom-specific behavior is therefore a deliberate, documented delta from its base compatibility profile rather than a separate divergent implementation.

  ### 8.3 Ruleset interpretation rules

  Gameplay variation is resolved in a single place: the compatibility profile and the option decoder. The rest of the engine should not need to know whether a specific effect came from vanilla, Boom, MBF, or RSDoom. Instead, the simulation code asks the profile for effective behavior, such as:

  - linedef and sector special interpretation
  - physics and movement tolerance
  - randomization and seeding behavior
  - menu and app-state policies
  - extended metadata handling
  - special backend behavior for rendering or audio only when it is presentation-only

  This keeps the sim, render, menu, and input systems from becoming a maze of `if compat_level == ...` checks. The compatibility rule is centralized where the semantics live, and all other subsystems consume a normalized result.

  ### 8.4 Demo metadata and external source-port compatibility

  Every demo identifies the profile and external inputs needed to reproduce it. A compatibility mode pair is only one part of that identity; flags, RNG algorithm, patches, UMAPINFO, skill, map, WAD fingerprints, and source-port/demo version are also relevant. Loading validates these inputs before constructing the active profile. This makes same-input replay deterministic without claiming that all demos sharing a complevel are interchangeable.

  Because the design uses a single controller plus explicit profile data, the compatibility contract stays fully inspectable: a demo can be validated by checking whether the selected profile and options lump match the required engine state and whether any inherited mode capabilities are present and consistent.

  ### 8.5 Data-model extensions

  Historical variants and RSDoom's own mode increase the metadata and behavior the engine can parse, but they do not create new engine-wide execution branches. They fit into:

  - raw WAD parsing and conversion layers
  - ruleset-aware interpretation in the compatibility profile and option decoder
  - optional metadata in the shared domain model or snapshot where the renderer needs it

  Examples:

  - generalized linedef specials
  - extended sector behavior
  - additional thing and weapon flags
  - DeHackEd/BEX behavior patches
  - UMAPINFO metadata as the priority extended map-info format, followed by other supported MAPINFO variants
  - alternate node formats normalized into a single domain representation
  - extended blockmaps, map formats, thing flags, weapons, states, and specials
  - RSDoom-specific compatibility features and gameplay decisions modeled as explicit profile behavior rather than ad hoc engine hacks

  The critical design rule is that behavior variation is resolved into the chosen profile, the decoded options payload, and the canonical simulation flow. Renderer, audio, menu, and input do not implement gameplay rules, though the render snapshot may expose profile-dependent visual data.

  ### 8.6 Implications for testing

  A single-controller design creates especially clear testing boundaries:

  - Profile tests: each compatibility profile can be instantiated and validated independently without cross-contamination by trait implementations or dispatch paths.
  - Determinism validation: run the same command stream against independent builds of the same released version on Linux and Windows and compare per-tic state hashes and final serialized state, requiring bit equality wherever practicable.
  - Cross-variant regression: play the same simple test level (e.g., E1M1 with scripted player input) under a selected profile and record traces. These traces will differ legitimately when the compatibility settings differ, but the behavior remains stable for the same profile and command stream.
  - External demo compatibility: collect demos from vanilla and DSDA-Doom reference configurations. Validate the complete profile and compare gameplay traces, state hashes, and completion outcomes; tolerances apply only to explicitly presentation-only data.
  - Malformed-input coverage: exercise invalid WADs, patches, metadata, demos, and resource-limit cases and require contextual errors without panics.
  - Diagnostics coverage: ensure replay traces, state hashes, and actor failures identify the tic, profile, map, and relevant input.
  - RSDoom-specific mode coverage: validate that any custom RSDoom feature or gameplay decision is covered by explicit tests and that its behavior is stable under the same profile and command stream.

  The testability benefit is significant: we can define a compact set of compatibility profile fixtures and validate them as ordinary data, rather than building a matrix of large trait-implementing types.

  ### 8.7 Maintainability and long-term tradeoff

  This approach prefers maintainability over micro-optimizing dispatch. The hot loop remains deterministic and straightforward, and the engine benefits from:

  - fewer `match` trees spread across system code
  - less duplicated compatibility logic in multiple structs
  - centralized behavior evaluation in one authoritative place
  - easier review when new modes are introduced

  The tradeoff is that compatibility checks may be done through a more explicit configuration lookup, which can be less optimal than perfect code specialization. That is acceptable here because the project values long-term correctness and maintenance over speculative performance micro-optimizations.

  ### 8.8 Recommended sequencing

  If the project later adds compatibility work, the natural order is:

  1. define the `CompatibilityProfile` and decoded `OptionsSet` model
  2. implement the single canonical simulation controller using that data
  3. define a stable per-tic command structure and make simulation consume it
  4. verify vanilla deterministic behavior with scripted input and recorded traces
  5. add cross-build determinism checks and replay diagnostics before optimizing
  6. layer in Boom, MBF, and MBF21 compatibility profiles, using DSDA-Doom as the reference where applicable
  7. define the RSDoom compatibility profile as an explicit engine-native ruleset inheriting lower-level behavior and adding its own intentional deltas
  8. prioritize DeHackEd/BEX, UMAPINFO, extended map formats, flags, states, weapons, and specials needed by those profiles
  9. collect and test against external source-port demos for conditional cross-port compatibility
  10. treat id24 as an ongoing compatibility target rather than a prerequisite; retain ZDoom-family semantics and ACS as explicit non-goals

  ---

  ## Summary

  The architecture is intentionally conservative and future-facing:

  - one authoritative simulation loop
  - explicit state machine rather than global flags
  - immutable snapshot publication for the renderer
  - clearly bounded audio and input layers
  - a ruleset and profile compatibility layer for historical DOOM variants and an explicit RSDoom compatibility mode that may define its own gameplay decisions and engine features
  - deterministic replay, state hashing, and actionable diagnostics
  - an exactness-oriented software renderer with explicitly scoped hardware approximations

  This keeps the port faithful to the spirit of the original game while making the implementation safer, more testable, and far easier to extend by human developers over time. It is also an intentional engineering choice: we preserve the actor model's semantics without pretending that multithreaded scheduling is required for correctness or determinism.
