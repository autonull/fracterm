# Fracterm Roadmap

Prioritized, consolidated backlog. Order of tiers = order of execution.
`P0` blocks everything; each `P#` roughly maps to a README milestone (M1–M9).
Prefer finishing a tier before starting the next.

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

- [ ] Groups: move/zoom/collapse/align/distribute/save-as-fragment
- [ ] Snapping: grid, edges, alignment guides, equal spacing
- [ ] Arrange commands: tile H/V, grid, cascade; align L/R/T/B; distribute H/V; zoom-to-group
- [ ] Multi-select + drag-and-drop reorder; z-order bring/send
- [ ] Layout persistence: nodes, transforms, styles, groups, z-order, profiles, selectors, bookmarks, modes
- [ ] Layout save/restore, versioning, auto-save, import/export
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