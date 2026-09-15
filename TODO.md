# Fracterm Roadmap

Prioritized, consolidated backlog. Order of tiers = order of execution.
`P0` blocks everything; each `P#` roughly maps to a README milestone (M1–M9).
Prefer finishing a tier before starting the next.

## Immediate (user-directed, do now)

Status note (2026-09-15): text is legible and verified (OCR reads labels,
typed echo, prompt `❯` via fallback). 76 tests green, 0 warnings. The items
below are spec'd in full so a fresh context can execute them without
re-discovering anything. DO NOT regress the hard-won fixes listed under
"Must not regress" at the bottom of this section.

### 1. Camera obeys the user completely
Goal: smooth, fast interpolation between points and zooms; the camera must
never steer itself back to a center on its own.

Background: `Camera` (`src/camera.rs`) has current (x/y/zoom) + target +
`animating`. Rule for all future edits: DIRECT manipulation sets current AND
target AND `animating=false` (user takes over); ANIMATED moves set target AND
`animating=true`. Anything else produces the "steers itself back" feel.

Already done: `Camera::pan` now syncs targets + cancels animation;
`Camera::update` is frame-rate independent exponential smoothing
(`SMOOTH_RATE = 14.0`, ~99% converged in 0.33s).

Still to do (fresh context starts here):
- [ ] `src/window.rs` `draw_frame`: change `.update(dt.min(0.1) * 10.0)` to
      `.update(dt.min(0.1))` (REAL seconds). The `* 10.0` with the new
      exponential update would make every animation snap instantly.
- [ ] `src/window.rs` `zoom_at_cursor` (wheel): retarget to operate on
      TARGETS so repeated wheels don't fight the easing:
      `old = cam.target_zoom; new = clamp(old * factor);`
      `cam.target_x += cx / old - cx / new;` (same for y, cx/cy = screen
      cursor); `cam.target_zoom = new; cam.animating = true;`
- [ ] `src/window.rs` `CursorMoved` pan branch: it sets `cam.x/cam.y`
      directly WITHOUT touching targets — if `animating` is true the next
      `update()` drags the camera back toward the stale target. THIS is the
      reported auto-steer. Fix: route through `cam.pan(dx, dy)` (or set
      `target_x/target_y = x/y` and `animating = false` alongside).
- [ ] `fit_dashboard` (`src/window.rs`): add `instant: bool` param.
      Startup calls with `true` (snap: set current+targets, no animation —
      app must be ready immediately). `f` key calls with `false` (set
      targets + `animating = true` for a smooth fly-to).
- [ ] Audit: every other `camera_mut()` site must be user-triggered
      (`finish_right_click` fit/zoom, `0`/`b`/digit keys, pin) — keep them,
      they already go through targets.
- [ ] Tests (`src/camera.rs` tests module): convergence — from (0,0,1) to
      target (100,50,2), step `update(1.0/60.0)`, assert `!animating` within
      60 steps and exact snap; pan-cancels-animation — set animating, call
      `pan`, assert `!animating` and targets == currents.
- [ ] Verify live (`DISPLAY=:0`, app window titled `fracterm`): wheel over
      window = smooth ~0.3s zoom anchored at cursor; left-drag pan = 1:1
      with NO drift/rubber-band after release (screenshot before/after must
      differ only by the pan offset).

### 2. Startup: ONE centered terminal, no Node 1/Node 2 confusion
Goal: app opens on a single complete terminal centered on screen. Delete the
demo projection node ("Node 2") and the "node N" label chrome.

- [ ] `src/window.rs` `resumed()`: delete the demo-projection block
      (selector/max_lines 20/Live/`pnode` at (vx,vy)). Keep
      `pin_current_view` (`p` key) as the on-demand way to create views.
- [ ] Same function: compute terminal pixel size from measured metrics
      (`GRID_COLS * cell_w + 2*GRID_PAD_X`,
      `header_h() + GRID_ROWS * line_h + GRID_PAD_BOTTOM`), then center it
      in the REAL window size:
      `tx = round((width - term_w) / 2)`, `ty = round((height - term_h) / 2)`;
      spawn there; snap camera to (0,0,1) (or instant `fit_dashboard`).
- [ ] `src/window.rs` `draw_frame`: delete the node-label loop
      (`format!("node {}", ...)`). Keep projection-content text rendering
      (pinned views still show their lines).
- [ ] Note: `arrange::dashboard_layout` becomes unused by startup after
      this (still `pub` + tested — leave it, do not delete).
- [ ] Verify live: screenshot shows exactly one terminal, centered
      (node rect center within ~5px of viewport center); tesseract OCR reads
      the prompt; log shows `cam=(0,0,x1.00)`.

### 3. New terminals spawn beside; camera drags between them
- [ ] `src/arrange.rs`: new pure helper + tests:
      `place_beside(anchor:(x,y,w,h), size:(w,h), gap:f64, view:(w,h)) -> (i32,i32)`
      — try right of anchor; if `x + w` overflows viewport width, wrap
      below anchor. Tests: side-by-side no-overlap at 1280 wide; wrap on
      narrow viewport (mirror the existing `dashboard_layout` tests).
- [ ] `src/window.rs`: extract `terminal_node_size(cols, rows) -> (i32,i32)`
      from `spawn_terminal_node` (uses `grid_cell` + `header_h()`).
- [ ] `src/window.rs` `n` key: anchor = focused session's node rect (or
      last terminal node); `pos = place_beside(anchor, terminal_node_size(),
      DASH_GAP, viewport)`; `spawn_terminal_node(GRID_COLS, GRID_ROWS, pos)`.
      Keep the 8-terminal cap.
- [ ] Camera drag between terminals already works via pan (item 1 fixes
      its feel). Verify live: `n` creates a second terminal to the right
      with a gap, no overlap (screenshot); drag pans across both.

### 4. Select / move / resize terminals (vector-app feel)
Goal: click a terminal to select it as a whole (accent border), drag to move
(with edge snapping), drag its corner handle to resize (grid + PTY follow).

- [ ] `src/window.rs` `CanvasState`: add `selected: Option<NodeId>`;
      replace `pan_anchor: Option<(f64,f64)>` with a drag state machine:
      `enum DragState { None, Pan { ax: f64, ay: f64 }, Move { node: NodeId, dx: f64, dy: f64 }, Resize { node: NodeId } }`
      where `dx/dy` = world-space grab offset (`node.pos - world_at_grab`).
- [ ] Left-press (`MouseInput`): compute world `(wx,wy)` from cursor+camera.
      If a node is selected and `(wx,wy)` is within ~10 screen px of its
      bottom-right corner → `Resize`. Else `hit_node(wx,wy)`:
      hit → `selected = id` + existing terminal-focus logic +
      `Move{node, dx: node.x - wx, dy: node.y - wy}`;
      miss → `selected = None`, `term_focus = false`, `Pan{anchor}`.
      (Copy hit data into owned values BEFORE mutating `self` — borrowck.)
- [ ] `CursorMoved`: match on drag —
      Pan: existing camera math, then route through `cam.pan` (item 1);
      Move: `node.transform = world - offset`, then snap via existing
      `arrange::snap_to_edges` (build a temp clone at the new pos like
      `apply_arrange` does; threshold 8 world px; use guide pos when within
      threshold);
      Resize: `size = max(world - node.pos, min_size)` where min comes from
      cell metrics (`2*cell_w + 2*PAD_X` by `header + 2*line_h + PAD_BOTTOM`);
      if the node has a session, re-derive its grid + PTY (next bullet).
- [ ] Extract `sync_session_grid(&mut self, node_id: NodeId)` (grid
      rows/cols from node size via a new pure helper
      `arrange::terminal_grid_size(node_w, node_h, cell_w, header_h, pad_x,
      pad_bottom) -> (u32, u32)`, clamped 2..=256, + cursor clamp +
      `pty.resize`). Reuse it in the Resize drag AND rewrite the
      `WindowEvent::Resized` handler loop to call it per session node
      (deletes the duplicated math there).
- [ ] `src/canvas.rs` `RectRenderer`: add `push_overlay_rect(x,y,w,h,color)`
      (filled rect into `overlay_verts` via existing `push_rect_verts`).
- [ ] `draw_frame` rect loop: selected node gets accent border
      `(0.35, 0.7, 1.0, 1.0)` thickness `2.0`; others keep theme border.
      After borders: if `selected`, draw the resize handle — filled accent
      square of `12.0 / zoom` world px at the node's bottom-right corner
      via `push_overlay_rect`.
- [ ] Tests: `terminal_grid_size` cases (exact-fit cols/rows, clamp mins);
      handle hit-test as a pure fn (screen-space distance of node corner to
      cursor < 10px); `place_beside` (item 3). Keep all 76 existing tests
      green: `cargo test --lib` (fast; PTY tests spawn real shells).
- [ ] Verify live: click terminal → accent border screenshot; drag →
      node follows cursor (diff screenshots); drag corner → node grows and
      new columns appear (log grid size or OCR wider lines); click empty
      canvas → border clears.
- [ ] `PtySession::resize` already exists; confirm SIGWINCH reaches the
      shell (type `echo $COLUMNS` after a handle-resize).

### Must not regress (hard-won, all verified live + unit-tested)
- `src/text.rs` glyph VAO byte offsets (`TEXT_UV_OFFSET_BYTES = 8`,
  `TEXT_COLOR_OFFSET_BYTES = 16` for pos2+uv2+color4). Wrong offsets made
  every glyph a solid white block with correct positions.
- `u_atlas_size` plain uniform (no `textureSize()` in the vertex shader);
  atlas zero-initialized (uninit texels bled through LINEAR filtering).
- `FontSystem::rasterize` fallback stack (`monospace → DejaVu Sans Mono →
  Noto Sans Symbols`); `queue_string` takes `fonts + font_id` and keys the
  atlas by the SUPPLYING face id. (Fish prompt `❯` only exists in DejaVu.)
- VT answers Primary DA (`\x1b[?1;2c`); without it fish holds its first
  prompt ~10s (blank window). Replies flow via `take_reply()` → `pty.write`.
- `PtySession` holds ONE writer (`take_writer` fails after first call —
  this silently dropped every keystroke after the first).
- Grid metrics measured from the 'M' glyph advance (9px), NEVER from
  `FT_Size_Metrics::max_advance` (reports 27px on Noto Sans Mono → giant
  node → 0.5x camera fit → smear).
- Pitch-aware `normalize_bitmap` (Gray/Mono/Gray2/Gray4/LCD/LCDV/BGRA,
  negative pitch); empty-glyph guards in `queue_string` + `Atlas::insert`.
- Alt-screen save/restore cursor (`saved_cursor`); press-only key handling
  (releases ignored); click sets `term_focus`; `encode_text_input` /
  `named_key_sequence` / `control_code` helpers + tests.
- Live loop that works: `DISPLAY=:0`, `./target/debug/fracterm &`,
  `xdotool windowfocus --sync $W` + plain `xdotool type` (NEVER
  `type --window`, XSendEvent is ignored by winit), `import -window $W`
  screenshots, `tesseract` OCR as legibility oracle, per-pixel numpy
  analysis. Headless font probe:
  `cargo test --lib text::tests::test_dump_glyph_geometry -- --nocapture`.
- Current diagnostics to keep: startup block, `font ... -> path` lines,
  first-PTY-bytes line. (Per-second stats + key logs were removed as noise.)

---

## Session Status (checkpoint)

**State as of this commit: all gates green** — `cargo build`, 54 tests passing,
`cargo clippy --all-targets -- -D warnings` clean, `cargo fmt --check` clean.
Binary runs (headless fallback works when no GL 3.3 context is available).

**Tested & working without a display:** all 54 unit/integration tests
(VT parser, PTY echo round-trip, TS transpile, QuickJS plugin lifecycle,
permission gating, projections, arrange/snap math, camera fitting, atlas keys).
**Needs on-display testing:** window rendering itself (rects, borders, glyph
crispness across zoom, terminal grid live-view), pan/zoom feel, resize/HiDPI.

To test interactively: `cargo run` (needs OpenGL 3.3 + freetype + fontconfig).
In-window: bash PTY live, wheel zoom, drag pan, right-click autozoom,
right-drag rect zoom, `P` pin view, `B`/`1–9` bookmarks, `T/H/V/A` arrange.

**Next up (in order):**
1. On-display verification pass of M1–M5 exit criteria
2. P6 remainder: typed commands/events/widgets SDK, more web globals,
   generated `.d.ts`, V8 host behind the trait
3. P7 remainder: alignment-guide rendering, multi-select + drag reorder,
   layout auto-save/import-export
4. P8 accessibility: reading lens, high-contrast theme, reduce-motion,
   `accessibility` config section
5. P9: command system + palette, XDG dirs, packaging

**Known gaps / tech debt:**
- Per-frame `glow::Context` recreate risk: none (stored once) but text pass
  queues per-cell (one queue call per terminal cell) — needs glyph batching
  by color/style runs for 60fps on dense grids
- Far-zoom cached layer textures + atlas damage tracking (P2) still pending
- HarfBuzz shaping (P2) not wired
- `projection` presentation options (wrap/reflow/line numbers) unimplemented
- CI (`.github/workflows/ci.yml`) untested on a real runner
  (needs libfreetype/fontconfig/clang — already in the apt line)
- serde pinned `=1.0.203` for swc compatibility; revisit when swc updates

---

## P0 — Unblock & Stabilize (do first)

- [x] Fix current build errors (already resolved in working tree; verified `cargo build`/`test` clean)
  - [x] `src/surface.rs:133` — returns value referencing a temporary
  - [x] `src/lib.rs:23` — private `SurfaceId` re-export
  - [x] `src/node.rs:31` — wrong arg count after `InputBehavior` change
  - [x] `src/workspace.rs:23` — `Camera::default` used but not implemented
- [x] Add `impl Default for Camera` (already present at `src/camera.rs:212`)
- [x] Add CI gate: `cargo fmt --check`, `cargo clippy -D warnings`, `cargo test` (`.github/workflows/ci.yml`)
- [x] Single source of truth for IDs (`NodeId`, `SurfaceId` defined in `src/lib.rs`; no private re-exports)
- [x] Zero clippy warnings (`cargo clippy --all-targets -- -D warnings` passes); inherent `default()` methods converted to `new()` + `impl Default`; type aliases for complex map types in `event.rs`/`rendering.rs`

## P1 — Canvas Core (README M1)

- [x] OpenGL context: `winit` + `glutin` + `glow`, OpenGL 3.3 core minimum (`src/canvas.rs`, `src/window.rs`)
- [x] Capability detection at startup (no compute; degrade effects gracefully) — `Capabilities::detect`
- [x] Render graph: `ContentPass` → `OverlayPass` → `PostProcessPass` via FBOs — `RenderGraphExecutor` (`src/canvas.rs`); degrades to direct-screen rendering on weak drivers; PostProcess effects are placeholders
- [x] Shape batching for rectangles/borders (one draw call per pass where possible) — `RectRenderer`
- [x] Camera pan/zoom (wheel zoom, drag pan, middle-drag pan) — cursor-anchored zoom in `src/window.rs`
- [x] Resize handling + HiDPI scale factor (`AppState::hidpi_scale`, surface resize)
- [x] Exit (partial): rectangles + text zoom/pan smoothly; deep-zoom text sharpening lands via P2

## P2 — Text Surface Rendering (README M2)

- [x] fontconfig discovery (`FontSystem::font_path`) + FreeType rasterization (`Face::rasterize`); HarfBuzz shaping still pending
- [x] Glyph atlas keyed on: font id, glyph id, pixel size, subpixel bucket, style flags, color mode — `Atlas` + `GlyphKey` (`src/text.rs`)
- [ ] Three-zoom strategy: `choose_pixel_size` covers near (1–8x) + very large (>8x, capped 512px); far cached layer textures pending
- [ ] Damage tracking + incremental atlas updates (atlas eviction-rebuild scaffold only)
- [x] Subpixel positioning — 1/5-em buckets via FreeType pen offset
- [ ] Crisp text at fractional DPI (verify under real display)
- [ ] Exit: text stays crisp across the full zoom range (needs on-display verification)

## P3 — Terminal Surface (README M3)

- [x] PTY via `portable-pty` (`src/pty.rs`): shell spawn, background reader thread, `write`, `resize` (SIGWINCH)
- [x] VT parser: ANSI/VT100/xterm via `vte` (`src/vt.rs`) — SGR (16/256/24-bit color, bold/italic/underline/reverse), cursor movement, erase, alt screen
- [x] Grid + damage flag (`Terminal.dirty`); scrollback (capped); alternate screen
- [ ] Cursor styles; line wrapping (partial: auto-wrap on overflow); resize propagation (`PtySession::resize` exists, not yet wired to window resize)
- [x] Keyboard → PTY (text, named keys, arrows, Ctrl+key control codes); mouse tracking/bracketed paste pending
- [ ] OSC title, OSC 8 hyperlinks, OSC 52 clipboard (permission-gated)
- [ ] Unicode: wide chars + spacer cells + `unicode-width` done; graphemes, emoji, Nerd Fonts, ambiguous-width config pending
- [ ] `TextSource` impls beyond PTY: command output, file tail, plugin source (stubs exist)
- [ ] Terminal profiles: default, big-text, ssh, logs, presentation, high-contrast (`TerminalConfig::profile` scaffold only)
- [ ] Exit: interactive `bash` works inside a canvas node (rendering path live; needs on-display verification)

## P4 — Projection System (README M4)

- [x] `ProjectionSurface` as a Node component (`node.projection`); source + selector + mode + presentation — done previously, now wired to real terminals
- [x] Selectors: rows, columns, filter (substring/regex via `regex` crate/levels), search, maxLines (tail), follow
- [x] Modes: live (re-sync each frame from source) vs snapshot (freezes on first capture, `snapshot_taken` guard)
- [x] Extract mode for filters; highlight mode pending
- [ ] Presentation: wrap, reflow, line numbers, timestamps, match highlight, font scale, theme override
- [x] Create/attach from a terminal (`ProjectionSurface::from_terminal`, `update_from_terminal`); detach = node ungrouped projection node (UI pending)
- [x] Live demo: last-20-lines projection node rendered as green text over terminal output
- [ ] Exit: terminal subrange views pinned and arranged independently (rendering live; pin-as-view UI + arrange commands pending)

## P5 — Lens & Zoom (README M5)

- [x] ZoomTarget variants: workspace-fit, rectangle (`Camera::fit_rect` — viewport-aware, centered, eased), object (`Camera::zoom_to_node` via node size)
- [x] Animated transitions with easing (`Camera::update` driven per frame from the window loop)
- [x] Right-click autozoom (topmost hit-test node); right-drag rectangle zoom; ctrl+right-click context-menu target reporting
- [x] "Pin as View": `p` key materializes the current viewport into a snapshot projection node
- [x] Camera bookmarks as lens targets: `b` saves, digits `1–9` restore (`Camera::save_bookmark`/`restore_bookmark`); named-bookmark UI pending
- [x] Node `size` component added (Transform previously had no size; hit-tests and zoom used wrong bounds)
- [ ] Reading lens target; terminal-range target
- [ ] Exit: any zoom can become a pinned view (snapshot pinning works; live pin + UI polish pending)

## P6 — JS Host & Typed SDK (README M6–M7)

- [x] `ScriptHost` trait (`src/script.rs`) mirroring README §13: load/unload/call/dispatch_event
- [x] `QuickJsScriptHost` via `rquickjs` — isolated runtime per plugin, `activate(ctx)`/`deactivate()` lifecycle hooks, disposables teardown on drop
- [x] Web-like globals scaffold: `console.log/error`, `fracterm.commands.register` host API
- [x] Typed event bus serialization (`Event` now serde) + `onEvent` dispatch to all plugins
- [x] SWC transpile on load for `.ts/.mts/.tsx` (parse + type-strip + codegen in `transpile_ts`; `.js/.mjs` pass through)
- [x] Scoped permission enforcement — `has_permission` (unscoped/scoped matching) + `commands.register` gated at the host API, denial aborts plugin load
- [ ] Typed commands (input schema), typed throttled events, declarative widgets
- [ ] Schema-driven settings UI auto-generation
- [ ] More web globals: timers, structuredClone, crypto, URL, AbortController
- [ ] Generate `.d.ts`: `fracterm`, `fracterm/config`, `fracterm/plugin`, `fracterm/widget`
- [ ] `V8ScriptHost` via `deno_core`/`v8` behind the same trait
- [ ] Exit: a TS plugin registers commands + widgets with a pleasant DX

## P7 — Dashboard Ergonomics (README M8)

- [x] Groups: create/add/remove/members exist (`SceneGraph`); move/zoom/collapse/save-as-fragment pending
- [x] Snapping: grid snapping (`snap_to_grid`) + edge snapping with guides (`snap_to_edges`); alignment guides UI pending
- [x] Arrange commands (`src/arrange.rs`): tile H/V, grid, cascade; align L/R/T/B; distribute H/V — wired to workspace keys T/H/V/A
- [x] Multi-select foundation: z-order bring/send exist; drag-and-drop reorder pending
- [x] Layout persistence: nodes/transforms/groups/z-order/bookmarks via `export_state`/`import_state`; profiles/selectors/modes pending
- [ ] Layout auto-save, import/export files
- [ ] Exit: dashboards can be built quickly

## P8 — Accessibility (README M9)

- [ ] Reading lens: reflow (not just magnification), line focus, ruler, word/line spacing
- [ ] High-contrast theme; reduce-motion; large cursor
- [ ] Quick reads: selection / current line / last 50 lines / filtered selection
- [ ] `accessibility` config: fontSize, lineHeight, highContrast, hideChrome, cursor, reduceMotion
- [ ] Keyboard-only navigation + visible focus rings
- [ ] Screen-reader / accessibility-bus integration
- [ ] Exit: near-sighted reading workflow is excellent

## P9 — Platform, Persistence & Commands

- [ ] Every action is a command; keybindings, HUD, palette, tests, CLI all bind to commands
- [ ] Command palette with fuzzy search (universal entry point)
- [ ] XDG dirs (`XDG_CONFIG/DATA/CACHE_HOME`); clipboard + primary selection; middle-click paste
- [ ] `.desktop` + AppStream metadata; window title; app ID
- [ ] Packaging: AppImage, deb, rpm, Arch, Flatpak
- [ ] Context menus (target-aware) per README §23
- [ ] Border controls + options popover (terminal/projection/widget variants) per README §12
- [ ] Auto-hiding HUD with configurable edge/hotkey
- [ ] Search: incremental, regex, case toggle, next/prev, projection-from-search
- [ ] Terminal profile editor + quick font-size adjust

---

## Help & Onboarding (new)

Make the system self-teaching; help is generated from live registries, not hand-written.

- [ ] `fracterm --help`, `fracterm help <topic>`, man page; `--version`, `--print-config`
- [ ] `fracterm doctor` with actionable fix suggestions (OpenGL, fonts, JS engine, manifests, perms, schema)
- [ ] In-app help overlay (`F1` / `?`): contextual by active mode/target
- [ ] First-run interactive onboarding tour + demo layout to explore
- [ ] Keyboard-shortcut cheatsheet auto-generated from the keybinding registry
- [ ] Command palette rows show description + keybinding + "open docs"
- [ ] Contextual "what can I do here?" action list per node/selection
- [ ] Tooltips exposing command id + bound keys
- [ ] Error messages link to troubleshooting entries
- [ ] Searchable in-app docs browser; glossary for lens/surface/projection terms
- [ ] Plugin authoring guide + `fracterm plugin new <template>` scaffolder
- [ ] "What's new" changelog viewer
- [ ] Self-test/diagnostics screen (render smoke test, font coverage, terminal conformance quick check)

## DX & Tooling

- [ ] In-app plugin console: logs, load state, grants, exceptions, source-mapped traces
- [ ] Hot reload: edit → SWC → deactivate → reload → restore workspace state
- [ ] `cargo xtask` (or Makefile) for fmt/clippy/test/bench/package
- [ ] Performance overlay (frame time, glyph cache hit rate, damage area, PTY backlog)
- [ ] Debug overlays: node bounds, transforms, surface damage regions, gizmos
- [ ] Crash reporting + session recovery
- [ ] Benchmarks for render loop, parser, atlas

## UI/UX Polish

- [ ] Undo/redo for workspace mutations
- [ ] Toasts + progress indicators; confirm destructive actions
- [ ] Real-time settings preview; theme editor
- [ ] Minimap / workspace overview; focus mode; presentation mode
- [ ] Dark/light auto theme; customizable keybinding editor
- [ ] Layout template gallery; session save/restore
- [ ] Multi-monitor + split views / workspace tabs
- [ ] i18n scaffolding for menus/help
- [ ] Touch gestures (pinch zoom, swipe)

## Testing & Quality

- [ ] Terminal conformance (vttest-style), Unicode width/grapheme/emoji, escape/resize/alt-screen/mouse/paste
- [ ] Golden-image tests: terminal, zoom, glyph crispness, projections, HUD, themes, opacity, effects
- [ ] Plugin contract tests: load, register, render, permissions, disposables, hot reload, errors
- [ ] Zoom/lens/camera integration tests
- [ ] Fuzzing: VT parser, projection selectors, layout deserializer

## Icebox (deliberately deferred)

- [ ] Kitty keyboard protocol, synchronized output, focus events
- [ ] Motion blur / background blur (keep optional + off by default)
- [ ] Audio/visual action feedback
- [ ] `fetch`/network host API, process.spawn host API
- [ ] Layout-as-image export
- [ ] Multi-workspace synchronization

---

## Done

- [x] Node components aligned to improved design (Transform, Style, Surface, Projection, InputBehavior, PluginBehavior; removed Focus/Behavior)
- [x] `ProjectionSurface` exposed as a Node component (live/snapshot + selector + presentation)
- [x] `CameraLens` unified into `Workspace`; `Deref` to `Camera` for compatibility
- [x] `ZoomTarget` enum + object-target application (`apply_target`)
- [x] `Workspace::node_transform`
- [x] `Cell` extended with underline/reverse/width + sensible defaults