# Fracterm — Phased Development Plan

Execution plan against the README Part III specification (Phases P1–P9 map
1:1 to spec milestones M1–M9 (§26); P0 is the unblock/stabilize gate).
Subsystem statuses and the source-module map live in README Part IV; the
regression guardrails this plan must respect are in Development Guardrails
below. Prefer finishing a phase before starting the next. Unfinished items
carry forward in-place, so this file is always the single resume point.

**Current state (2026-09-16): all gates green** — `cargo build`, 147 tests
passing, `cargo clippy --all-targets -- -D warnings` clean,
`cargo fmt --check` clean. Fish startup fixed (~1s prompt via terminal-query
replies); caret/title/modes live-verified on :0. Binary runs on-display (X :0, 1280x720 Luscombe);
headless fallback intact. Text legible and verified (OCR reads labels, typed
echo, prompt `❯` via fallback).

Display-verified this session: single full-bleed terminal (aspect-fitted grid
+ camera fit), typed echo + `$COLUMNS`=103, select accent border + handle,
`n`-spawn placement, pan with zero post-release drift, no `node N` labels.

Tested & working without a display: VT parser, PTY echo round-trip, TS
transpile, QuickJS plugin lifecycle, permission gating, projections,
arrange/snap math, camera fitting, atlas keys.
Needs on-display testing: window rendering (rects, borders, glyph crispness
across zoom, terminal grid live-view), pan/zoom feel, resize/HiDPI.

---

## Phase P0 — Unblock & Stabilize ✅ (complete)

- [x] Fix build errors: `src/surface.rs:133` temp reference; `src/lib.rs:23`
      private `SurfaceId` re-export; `src/node.rs:31` arg count after
      `InputBehavior` change; `src/workspace.rs:23` `Camera::default`.
- [x] `impl Default for Camera` (`src/camera.rs`).
- [x] CI gate: `cargo fmt --check`, `cargo clippy -D warnings`, `cargo test`
      (`.github/workflows/ci.yml`).
- [x] Single source of truth for IDs (`NodeId`, `SurfaceId` in `src/lib.rs`).
- [x] Zero clippy warnings; inherent `default()` → `new()` + `impl Default`;
      type aliases for complex map types (`event.rs`, `rendering.rs`).

## Phase P1 — Canvas Core (README M1)

- [x] OpenGL context: `winit` + `glutin` + `glow`, 3.3 core min
      (`src/canvas.rs`, `src/window.rs`).
- [x] Capability detection (`Capabilities::detect`); degrade effects
      gracefully.
- [x] Render graph: ContentPass → OverlayPass → PostProcessPass via FBOs
      (`RenderGraphExecutor`); degrades to direct-screen on weak drivers;
      PostProcess placeholders.
- [x] Shape batching (`RectRenderer`).
- [x] Camera pan/zoom — cursor-anchored zoom; resize + HiDPI scale factor.
- [x] Camera correctness (session 2026-09-15, all unit-tested):
  - [x] `draw_frame` uses REAL seconds: `.update(dt.min(0.1))`.
  - [x] `zoom_at_cursor` operates on TARGETS (repeated wheels don't fight
        the easing).
  - [x] `CursorMoved` pan routes through `cam.pan` (kills auto-steer —
        replaces raw `cam.x/cam.y` writes with stale targets;
        `pan_anchor` state was absorbed into `DragState::Pan`).
  - [x] `fit_dashboard(instant)`: startup snaps; `f` key flies smoothly.
  - [x] Audit: all `camera_mut()` sites user-triggered or target-based.
  - [x] Tests: convergence (exact snap within 60 steps), pan-cancels-
        animation.
  - [ ] Verify live: wheel zoom smooth ~0.3s anchored at cursor (pan half
        done: middle-drag verified, no drift/rubber-band).

**Camera rule (all future edits):** DIRECT manipulation sets current AND
target AND `animating=false`; ANIMATED moves set target AND
`animating=true`. Anything else produces "steers itself back".

## Phase P2 — Text Surface Rendering (README M2)

- [x] fontconfig discovery + FreeType rasterization.
- [x] Glyph atlas keyed on: font id, glyph id, pixel size, subpixel bucket,
      style flags, color mode (`Atlas` + `GlyphKey`).
- [x] Subpixel positioning — 1/5-em buckets via FreeType pen offset.
- [x] `choose_pixel_size`: near (1–8x) + very large (>8x, capped 512px).
- [ ] Far-zoom cached layer textures + damage tracking + incremental atlas
      updates (eviction-rebuild scaffold only).
- [ ] Crisp text at fractional DPI (verify under real display).
- [ ] HarfBuzz shaping.
- [ ] Exit: text stays crisp across the full zoom range (on-display verify).

## Phase P3 — Terminal Surface (README M3)

- [x] PTY via `portable-pty`: shell spawn, background reader, `write`,
      `resize` (SIGWINCH).
- [x] VT parser: ANSI/VT100/xterm via `vte` — SGR (16/256/24-bit, bold/
      italic/underline/reverse), cursor movement, erase, alt screen.
- [x] Grid + damage flag; scrollback (capped); alternate screen.
- [x] Keyboard → PTY (text, named keys, arrows, Ctrl+key control codes).
- [x] Startup: ONE centered terminal — demo-projection node and "node N"
      labels deleted; `pin_current_view` (`p` key) kept as the on-demand way
      to create views; `arrange::dashboard_layout` unused by startup but
      stays `pub` + tested. Aspect-fitted grid (`ideal_grid_for_view`) +
      centering `fit_rect` snap (e.g. cam 169,95,1.36 at 1280x720).
- [x] New terminals spawn beside (`place_beside`, wraps below on narrow
      viewports); 8-terminal cap.
- [x] Select/move/resize (session 2026-09-15):
  - [x] `DragState { None, Pan, Move, Resize }` state machine.
  - [x] Hit-test → select (accent border + 12px/zoom resize handle via
        `push_overlay_rect`).
  - [x] `sync_session_grid`: grid from node size (`terminal_grid_size`,
        clamp 2..=256) + cursor clamp + `pty.resize`; reuses in Resize drag
        AND `WindowEvent::Resized` handler (deletes duplicated math there).
  - [x] Extract `terminal_node_size(cols, rows)` from `spawn_terminal_node`
        (uses `grid_cell` + `header_h()`); `push_overlay_rect` added to
        `RectRenderer`.
  - [x] Copy hit data into owned values BEFORE mutating `self` (borrowck);
        Move grabs world-space offset (`node.pos - world_at_grab`); resize
        min-size from cell metrics; snap threshold 8 world px via temp-clone
        like `apply_arrange`.
  - [x] `terminal_grid_size` + handle-hit-test + `place_beside` unit tests.
- [x] Cursor visibility (?25) + style (DECSCUSR block/bar/underline +
      blink) + autowrap (?7) + bracketed-paste mode (?2004) + mouse-mode
      tracking (?1000/1002/1003/1006); caret rendered as overlay rect shaped
      by style, blinked at ~530ms, hidden on ?25l (all live-verified:
      `tput civis` → 0 caret px, `ESC[5 q` → 2px bar, restore → block).
- [x] Terminal query replies (no more timeout stalls): Primary DA (was),
      DSR `CSI 5 n`/`CSI 6 n` (CPR), kitty `CSI ? u` → `?0u`, XTVERSION
      `CSI > 0 q`, OSC 11 background (`rgb:0b0b/0d0d/1212`), XTGETTCAP
      `DCS + q` → `DCS 0 + r` (Sixel `q`-final payloads guarded by `+`).
      Fish first prompt now ~1s (was ~10–14s intermittently).
- [x] OSC 0/2 window title applied to the winit window on change
      (live: fish `~ - fish` title appears; falls back to `fracterm`).
- [ ] Line wrapping edge cases; resize propagation verify (`echo $COLUMNS`
      after handle-resize).
- [x] Mouse event forwarding to the child (session 2026-09-16):
  - [x] Per-mode tracking (`?1000` clicks, `?1002` drags, `?1003` bare
        motion; SGR `?1006` flag) via `MouseMode::set_mode`.
  - [x] `MouseMode::encode`: SGR (`ESC[<Cb;Cx;Cy M/m`, 1-based) and legacy
        X10 (`ESC[M` +32, clamped to 255) with shift/alt/ctrl/motion bits;
        `wants` gates press/release/wheel/motion per mode.
  - [x] Window wiring: unshifted press on a reporting terminal goes to its
        PTY (focus + select follow, `forwarding` owns the mouse until left
        release); drag motion reports under 1002/1003; release report ends
        it. Shift-click forces host select/move; resize handle always wins.
  - [ ] Wheel still zooms (host nav); forwarding wheel to the child (e.g.
        vim/less scroll) is a follow-up needing a scroll-vs-zoom decision.
- [x] Paste path (session 2026-09-16, live-verified on :0):
  - [x] `arboard` clipboard read (+ X11 primary via `LinuxClipboardKind`);
        failures (headless/empty) are no-ops.
  - [x] Ctrl+Shift+V / Shift+Insert paste clipboard into the focused
        terminal, framed by `bracket_paste` (?2004).
  - [x] Middle-click (no drag) pastes primary at the cursor session;
        movement past 5px becomes a pan as before.
  - [x] Live: `PASTE789` via Ctrl+Shift+V and `MID123` via middle-click
        landed in the prompt; Return executed the line; middle-drag pan
        stayed responsive.
- [ ] Copy side needs text selection (left-drag select still spec).
- [ ] OSC 8 hyperlinks, OSC 52 clipboard (permission-gated).
- [ ] Unicode: graphemes, emoji, Nerd Fonts, ambiguous-width config (wide
      chars + spacer cells done).
- [ ] `TextSource` impls beyond PTY: command output, file tail, plugin
      source (stubs exist).
- [ ] Terminal profiles: default/big-text/ssh/logs/presentation/high-contrast
      (scaffold only).

**Live-verify queue (P3):**
- [x] Caret visible by default; `?25l` hides, `?25h` restores; bar/underline
      shapes render; OSC title reaches the window title.
- [x] `Return`/Ctrl+chords execute (verified `echo` round-trips repeatedly).
- [ ] Move-drag follows cursor (diff screenshots); drag corner → node grows,
      new columns appear; click empty canvas → border clears.
- [ ] Mouse forward live: `vim` click-moves cursor, tmux selects pane,
      drag reports under `?1002`, Shift-click still moves the node.
- [ ] Post-resize SIGWINCH: handle-resize then `echo $COLUMNS`.
- [ ] Side-by-side (non-wrap) `n`-spawn on a wider viewport.

## Phase P4 — Projection System (README M4)

- [x] `ProjectionSurface` node component: source + selector + mode +
      presentation.
- [x] Selectors: rows, columns, filter (substring/regex/levels), search,
      maxLines (tail), follow.
- [x] Modes: live (re-sync per frame) vs snapshot (`snapshot_taken` guard).
- [x] Extract filter mode; highlight mode pending.
- [x] `ProjectionSurface::from_terminal` / `update_from_terminal`; detach =
      ungrouped projection node.
- [ ] Presentation: wrap, reflow, line numbers, timestamps, match highlight,
      font scale, theme override.
- [ ] Pin-as-view UI + arrange commands for projections.
- [ ] Exit: subrange views pinned and arranged independently.

## Phase P5 — Lens & Zoom (README M5)

- [x] `ZoomTarget`: workspace-fit, rectangle (`Camera::fit_rect` —
      viewport-aware, centered, eased), object (`zoom_to_node`).
- [x] Animated transitions with easing (frame-rate independent).
- [x] Right-click autozoom; right-drag rect zoom; ctrl+right-click context
      menu target.
- [x] "Pin as View": `p` materializes viewport into snapshot projection node.
- [x] Camera bookmarks: `b` saves, digits restore; named-bookmark UI pending.
- [x] Fix bookmark naming: `b` saves to rotating slots `bm0`–`bm9`
      (`Camera::save_next_bookmark`), digits restore, `save_bookmark`
      upserts instead of stacking duplicates; `0` restores `bm0` when
      present, else zooms to workspace fit. Named-bookmark UI still pending.
- [x] Node `size` component (hit-tests/zoom used wrong bounds before).
- [x] Reading + terminal-range targets (session 2026-09-16):
  - [x] `CameraLens::zoom_to_viewport`: WorkspaceFit (content bounds +
        margin, capped at 1.0), Object, Rectangle, TerminalRange (whole-node
        fit — grid dims live in window sessions, not the workspace),
        Projection (node/surface id, else first projection node), Reading
        (source-surface fit + `reading_mode` flag). Unknown ids are no-ops.
        All branches are animated (target + `animating`), per the camera rule.
  - [x] `terminal_range_rect` pure helper: grid-range → node sub-rect with
        clamping; window code with live `Terminal` dims zooms to it as a
        `Rectangle`. `workspace_content_bounds` shared with dashboard fit.
- [ ] Exit: any zoom can become a pinned view (live pin + UI polish).

## Phase P6 — JS Host & Typed SDK (README M6–M7)

- [x] `ScriptHost` trait mirroring spec §13.
- [x] `QuickJsScriptHost` via `rquickjs` — isolated runtime per plugin,
      activate/deactivate lifecycle, disposables teardown.
- [x] Web globals scaffold: `console.log/error`,
      `fracterm.commands.register`.
- [x] Typed event bus (serde `Event`) + `onEvent` dispatch.
- [x] SWC transpile on load for `.ts/.mts/.tsx` (`transpile_ts`: parse +
      type-strip + codegen); `.js/.mjs` pass through.
- [x] Scoped permission enforcement (`has_permission`), denial aborts load.
- [x] Throttled events enforced (`Subscription::poll_throttle` with
      interior mutability, so `emit(&self)` rate-limits; explicit windows
      win, else spec `default_throttle` for `TerminalOutput`/`ZoomChanged`;
      `emit_for_plugin` honors the same windows).
- [x] Command input type checking (`check_param_type`: string/number/
      boolean/array/object/any; unknown type names pass leniently).
- [x] Declarative widgets SDK surface (`WidgetDefinition`: state init +
      interval timers + view fn; `LiveWidget` owns state, `tick(now_ms)`
      fires due timers, renders display lists via `WidgetRenderer`;
      registry end-to-end collection tested).
- [x] Setting validation (`SettingSchema::validate_value`: type, numeric
      min/max, string enum; `resolve_settings` fills defaults, rejects
      unknown keys and invalid values/defaults).
- [x] Schema-driven settings UI data (`settings_ui_rows`: stable
      key-sorted `SettingUiRow`s with kind/default/min/max/options for the
      border options menu); menu rendering itself pending.
- [x] Web globals (README §14): `setTimeout`/`setInterval`/
      `clearTimeout`/`clearInterval` (JS due-queue drained by host
      `poll_timers`), `queueMicrotask`, `structuredClone` (JSON
      round-trip), `TextEncoder`/`TextDecoder` (UTF-8), `URL`/
      `URLSearchParams` (subset), `crypto.randomUUID` (v4),
      `AbortController`/`AbortSignal`, `console.warn/info/debug` aliases.
      `poll_all_timers` drains every plugin in one call (frame-loop entry
      point); calling it from `draw_frame` still pending — `CanvasState`
      holds no script host yet.
- [x] Generate `.d.ts` (`src/dts.rs`): `fracterm`, `fracterm/config`,
      `fracterm/plugin`, `fracterm/widget` — generated from the Rust API
      surface (defineConfig/definePlugin/defineWidget, commands, events,
      permissions, settings schemas, UiBuilder shapes).
- [ ] `V8ScriptHost` via `deno_core`/`v8` behind the same trait.
- [ ] Exit: a TS plugin registers commands + widgets with pleasant DX.

## Phase P7 — Dashboard Ergonomics (README M8)

- [x] Groups: create/add/remove/members; move/zoom/collapse/fragment pending.
- [x] Snapping: grid (`snap_to_grid`) + edge with guides (`snap_to_edges`).
- [x] Arrange commands: tile H/V, grid, cascade; align L/R/T/B; distribute
      H/V (keys T/H/V/A).
- [x] Multi-select foundation: z-order bring/send; drag reorder pending.
- [x] Layout persistence (v2 format): full nodes in z-order, explicit
      z-order (`set_z_order`), groups, parent→child edges, camera +
      bookmarks + slot via `export_state`/`import_state`; `save_to_file`/
      `load_from_file` helpers; v1 imports camera-only; malformed entries
      skipped; round-trip + file + v1-compat tests. Live PTY sessions are
      not persisted — terminal nodes restore as structure, shells re-spawn.
- [ ] Alignment-guide rendering, multi-select drag reorder, layout
      auto-save; profiles/selectors/modes in layouts.
- [ ] Exit: dashboards can be built quickly.

## Phase P8 — Accessibility (README M9)

- [ ] Reading lens: reflow, line focus, ruler, word/line spacing.
- [ ] High-contrast theme; reduce-motion; large cursor.
- [ ] Quick reads: selection / current line / last 50 lines / filtered.
- [ ] `accessibility` config section (fontSize, lineHeight, highContrast,
      hideChrome, cursor, reduceMotion).
- [ ] Keyboard-only navigation + visible focus rings.
- [ ] Screen-reader / accessibility-bus integration.
- [ ] Exit: near-sighted reading workflow is excellent.

## Phase P9 — Platform, Persistence & Commands

- [ ] Every action is a command; keybindings/HUD/palette/tests/CLI bind to
      commands.
- [ ] Command palette with fuzzy search.
- [ ] XDG dirs; clipboard + primary selection; middle-click paste.
- [ ] `.desktop` + AppStream metadata; window title; app ID.
- [ ] Packaging: AppImage, deb, rpm, Arch, Flatpak.
- [ ] Target-aware context menus (spec §23); border controls + options
      popover (spec §12); auto-hiding HUD.
- [ ] Search: incremental, regex, case toggle, next/prev,
      projection-from-search.
- [ ] Terminal profile editor + quick font-size adjust.

---

## Next Up (execution order)

1. Remaining P1/P3 live gaps: wheel-zoom timing/anchoring, move-drag with
   edge-snap feel, handle-resize → grid+PTY follow (`echo $COLUMNS` proves
   SIGWINCH), side-by-side `n`-spawn on a wider viewport.
2. On-display verification pass of M1–M5 exit criteria.
3. P6 remainder: typed commands/events/widgets SDK, more web globals,
   generated `.d.ts`, V8 host behind the trait.
4. P7 remainder: alignment-guide rendering, multi-select drag reorder,
   layout auto-save/import-export.
5. P8 accessibility: reading lens, high-contrast theme, reduce-motion,
   `accessibility` config.
6. P9: command system + palette, XDG dirs, packaging.

---

## Cross-cutting: Help & Onboarding

Self-teaching; help generated from live registries, not hand-written.

- [ ] `fracterm --help`, `fracterm help <topic>`, man page; `--version`,
      `--print-config`.
- [ ] `fracterm doctor` with actionable fix suggestions.
- [ ] In-app help overlay (`F1`/`?`): contextual by mode/target.
- [ ] First-run onboarding tour + demo layout.
- [ ] Cheatsheet auto-generated from keybinding registry.
- [ ] Palette rows show description + keybinding + "open docs".
- [ ] Contextual "what can I do here?" per node/selection.
- [ ] Tooltips exposing command id + bound keys.
- [ ] Error messages link to troubleshooting.
- [ ] Searchable in-app docs browser; glossary (lens/surface/projection).
- [ ] Plugin authoring guide + `fracterm plugin new <template>`.
- [ ] "What's new" changelog viewer.
- [ ] Self-test screen (render smoke test, font coverage, VT quick check).

## Cross-cutting: DX & Tooling

- [ ] In-app plugin console: logs, load state, grants, exceptions,
      source-mapped traces.
- [ ] Hot reload: edit → SWC → deactivate → reload → restore state.
- [ ] `cargo xtask` (or Makefile) for fmt/clippy/test/bench/package.
- [ ] Performance overlay (frame time, glyph cache hit rate, damage area,
      PTY backlog).
- [ ] Debug overlays: node bounds, transforms, damage regions, gizmos.
- [ ] Crash reporting + session recovery.
- [ ] Benchmarks: render loop, parser, atlas.

## Cross-cutting: UI/UX Polish

- [ ] Undo/redo for workspace mutations.
- [ ] Toasts + progress indicators; confirm destructive actions.
- [ ] Real-time settings preview; theme editor.
- [ ] Minimap / workspace overview; focus mode; presentation mode.
- [ ] Dark/light auto theme; keybinding editor.
- [ ] Layout template gallery; session save/restore.
- [ ] Multi-monitor + split views / workspace tabs.
- [ ] i18n scaffolding; touch gestures (pinch zoom, swipe).

## Cross-cutting: Testing & Quality

- [ ] Terminal conformance (vttest-style), Unicode width/grapheme/emoji,
      escape/resize/alt-screen/mouse/paste.
- [ ] Golden-image tests: terminal, zoom, glyph crispness, projections, HUD,
      themes, opacity, effects.
- [ ] Plugin contract tests: load, register, render, permissions,
      disposables, hot reload, errors.
- [ ] Zoom/lens/camera integration tests.
- [ ] Fuzzing: VT parser, projection selectors, layout deserializer.

## Icebox (deliberately deferred)

- [ ] Kitty keyboard protocol, synchronized output, focus events.
- [ ] Motion blur / background blur (optional + off by default).
- [ ] Audio/visual action feedback.
- [ ] `fetch`/network host API, process.spawn host API.
- [ ] Layout-as-image export.
- [ ] Multi-workspace synchronization.

---

## Development Guardrails — Must Not Regress

Hard-won, all verified live + unit-tested. Any refactor must preserve these.

- `src/text.rs` glyph VAO byte offsets (`TEXT_UV_OFFSET_BYTES = 8`,
  `TEXT_COLOR_OFFSET_BYTES = 16` for pos2+uv2+color4). Wrong offsets made
  every glyph a solid white block with correct positions.
- `u_atlas_size` plain uniform (no `textureSize()` in the vertex shader);
  atlas zero-initialized (uninit texels bled through LINEAR filtering).
- `FontSystem::rasterize` fallback stack (monospace → DejaVu Sans Mono →
  Noto Sans Symbols); `queue_string` takes `fonts + font_id` and keys the
  atlas by the SUPPLYING face id. (Fish prompt `❯` only exists in DejaVu.)
- VT answers Primary DA (`\x1b[?1;2c`); without it fish holds its first
  prompt ~10s (blank window). Replies flow via `take_reply()` → `pty.write`.
- `PtySession` holds ONE writer (`take_writer` fails after first call —
  silently dropped every keystroke after the first).
- Grid metrics measured from the 'M' glyph advance (9px), NEVER from
  `FT_Size_Metrics::max_advance` (reports 27px on Noto Sans Mono → giant
  node → 0.5x camera fit → smear).
- Pitch-aware `normalize_bitmap` (Gray/Mono/Gray2/Gray4/LCD/LCDV/BGRA,
  negative pitch); empty-glyph guards in `queue_string` + `Atlas::insert`.
- Alt-screen save/restore cursor (`saved_cursor`); press-only key handling;
  click sets `term_focus`; `encode_text_input` / `named_key_sequence` /
  `control_code` helpers + tests.
- Camera rule: direct manipulation sets current+target+`animating=false`;
  animated moves set target+`animating=true`.
- Keep `arrange::dashboard_layout` `pub` + tested (unused by startup, not
  dead).

## Live Verification Procedure

Proven loop: `DISPLAY=:0`, `./target/debug/fracterm &`,
`xdotool windowfocus --sync $W` + plain `xdotool type` (NEVER
`type --window` — XSendEvent is ignored by winit), `import -window $W`
screenshots, `tesseract` OCR as legibility oracle, per-pixel numpy analysis.
Headless font probe:
`cargo test --lib text::tests::test_dump_glyph_geometry -- --nocapture`.

Lessons 2026-09-16 (don't re-learn):
- Focus/keys at the WM FRAME are lost: find the frame by geometry
  (`1290x754+315+163`), then the client inside it (`("fracterm"` class,
  1280x720) — `windowactivate` the CLIENT id and confirm with
  `getwindowfocus` + a typed-char probe before trusting key delivery.
- `kill <PID>` only (never `pkill` — hangs); wait ~2s after kill before
  relaunch or the new instance may map no window.
- Prefer quoteless/shiftless typed commands (`tput civis`, `echo X`);
  for exact bytes write a script file and run `sh /tmp/x.sh`.
- Fish redraws its prompt (and `?25h`) after every command: observe
  hidden/styled-cursor states mid-`sleep`, not after.
- A stuck-pending line + dead Returns usually means fish went multiline
  (lost quote char) — `ctrl+c` resets to a fresh prompt.

## Environment Notes

- Window spawns at screen offset — add window pos to window-rel coords for
  xdotool clicks.
- NEVER `pkill` here (hangs) — use `kill <PID>`.
- Don't chain `build && launch &` in one shell call (tool waits on the
  pipe) — build, then launch, in separate calls.
- Toolchain: clippy 1.94 flags `needless_return` on match-arm tails and
  `needless_range_loop`; `grid_cell_origin` (8 args) carries `#[allow]`.
- serde pinned `=1.0.203` for swc compatibility; revisit when swc updates.
- CI (`.github/workflows/ci.yml`) untested on a real runner (needs
  libfreetype/fontconfig/clang — already in the apt line).

## Known Gaps / Tech Debt

- Text pass queues per-cell (one queue call per terminal cell) — needs
  glyph batching by color/style runs for 60fps on dense grids.
- Per-frame `glow::Context` recreate risk: none (stored once).
- Full-bleed startup ⇒ no empty canvas: left-drag always grabs a node;
  pan is middle-drag only. Consider space-pan / Alt-drag fallback.
- Fish first prompt FIXED 2026-09-16 (was ~14s blank): fish's startup query
  burst (kitty `?u`, XTVERSION, OSC 11, XTGETTCAP, CPR) now gets instant
  replies; prompt lands ~1s after launch, verified across relaunches.
- Startup diagnostics to keep: startup block, `font ... -> path` lines,
  first-PTY-bytes line. (Per-second stats + key logs removed as noise.)

## Done Log
 
- Node components aligned to improved design (Transform, Style, Surface,
  Projection, InputBehavior, PluginBehavior; removed Focus/Behavior).
- `ProjectionSurface` exposed as a Node component (live/snapshot + selector
  + presentation).
- `CameraLens` unified into `Workspace`; `Deref` to `Camera`.
- `ZoomTarget` enum + object-target application (`apply_target`).
- `Workspace::node_transform`.
- `Cell` extended with underline/reverse/width + sensible defaults.
 
---
 
## Refactoring Session (2026-09-15) — Core Abstractions Complete
 
**Major refactoring to prepare for TODO.md implementation:**
 
- [x] **Command System** (`src/command.rs`): Typed commands with `CommandInputSchema` validation, `CommandRegistry` with keybinding integration, `CommandContext` for execution context, `CommandValue` for JSON-serializable params.
- [x] **Plugin System** (`src/plugin.rs`): Typed SDK with `PluginManifest`, `PluginContext`, `WidgetRenderer` trait (display lists only), `WidgetEvent`/`WidgetRenderContext`, `UiBuilder` immediate-mode API, `Disposable` for cleanup, `PluginManager` with mutex-protected host.
- [x] **TypeScript Config** (`src/config.rs`): `Config::load_from_ts()` with SWC transpilation, `eval_ts_config()` via QuickJS, profile support, XDG config directory resolution.
- [x] **Input System** (`src/input.rs`): `InteractionMode` enum (Workspace/Terminal/Reading/Dashboard), `InputManager` routing to mode-specific handlers (`WorkspaceInputHandler`, `TerminalInputHandler`, `ReadingInputHandler`, `DashboardInputHandler`), `ContextMenuBuilder` with target-aware menus.
- [x] **Widget Display List Pipeline** (`src/widget.rs`): `WidgetDisplayList`, `WidgetRegistry`, `render_widget_display_list()`, `collect_widget_display_lists()` bridging plugin display lists → canvas renderer.
- [x] **Event System** (`src/event.rs`): `EventBus` with throttling (`subscribe_throttled`), permission-gated emission (`emit_for_plugin`), `EventHistory` for replay, `EventFilter` for selective subscription.
- [x] **Layout Persistence**: Full `Serialize`/`Deserialize` on `Node`, `Transform`, `Theme`, `Color`, `ProjectionSurface`, `Permission`, `InputBehavior`, `PluginBehavior`, ID types. `Workspace::export_state`/`import_state` round-trips scene graph.
- [x] **Reading Mode / Accessibility primitives**: `ReadingModeConfig` in config, `InteractionMode::Reading`, `ReadingInputHandler`, context menu items for reading actions.
- [x] **World coordinates → f64**: `Transform{x,y}` now `f64`, `Node::size` now `(f64,f64)`, `Rect` now `f64`, `arrange.rs` functions return `f64` coords, `snap_to_edges` uses `f64` threshold. Eliminates sub-pixel aliasing at fractional zoom.
 
**Tests**: 99 passing (was 83). Build clean, clippy clean (warnings only on unused vars).
 
**Remaining for fresh context:**
- On-display verification of P1–P5 exit criteria
- P6: typed commands/events/widgets SDK completion, more web globals, `.d.ts` generation, V8 host
- P7: alignment-guide rendering, multi-select drag reorder, layout auto-save
- P8: reading lens reflow, high-contrast theme, reduce-motion
- P9: command palette, XDG dirs, packaging
