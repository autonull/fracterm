# Fracterm Roadmap

Prioritized, consolidated backlog. Order of tiers = order of execution.
`P0` blocks everything; each `P#` roughly maps to a README milestone (M1–M9).
Prefer finishing a tier before starting the next.

---

## P0 — Unblock & Stabilize (do first)

- [ ] Fix current build errors:
  - [ ] `src/surface.rs:133` — returns value referencing a temporary
  - [ ] `src/lib.rs:23` — private `SurfaceId` re-export
  - [ ] `src/node.rs:31` — wrong arg count after `InputBehavior` change
  - [ ] `src/workspace.rs:23` — `Camera::default` used but not implemented
- [ ] Add `impl Default for Camera` (or route through `Camera::new`)
- [ ] Add CI gate: `cargo fmt --check`, `cargo clippy -D warnings`, `cargo test`
- [ ] Single source of truth for IDs (`NodeId`, `SurfaceId`, …) — stop re-exporting privates

## P1 — Canvas Core (README M1)

- [ ] OpenGL context: `winit` + `glutin` + `glow`, OpenGL 3.3 core minimum
- [ ] Capability detection at startup (no compute; degrade effects gracefully)
- [ ] Render graph: `ContentPass` → `OverlayPass` → `PostProcessPass` via FBOs
- [ ] Shape batching for rectangles/borders (one draw call per pass where possible)
- [ ] Camera pan/zoom (wheel zoom, drag pan, middle-drag pan)
- [ ] Resize handling + HiDPI scale factor
- [ ] Exit: rectangles + text zoom/pan smoothly at 60 fps

## P2 — Text Surface Rendering (README M2)

- [ ] fontconfig discovery + FreeType rasterization + HarfBuzz shaping
- [ ] Glyph atlas keyed on: font id, glyph id, pixel size, subpixel bucket, style flags, color mode
- [ ] Three-zoom strategy: cached layer textures (far) → direct glyphs (near) → SDF/high-res (very large)
- [ ] Damage tracking + incremental atlas updates
- [ ] Subpixel positioning; crisp text at fractional DPI
- [ ] Exit: text stays crisp across the full zoom range

## P3 — Terminal Surface (README M3)

- [ ] PTY via `portable-pty` (async with `tokio`)
- [ ] VT parser: ANSI/VT100/xterm, SGR, 8/16/24-bit color
- [ ] Grid + damage tracking; scrollback; alternate screen
- [ ] Cursor styles; line wrapping; resize propagation
- [ ] Keyboard → PTY; mouse tracking (xterm/vt200/sgr); bracketed paste
- [ ] OSC title, OSC 8 hyperlinks, OSC 52 clipboard (permission-gated)
- [ ] Unicode: wide chars, graphemes, emoji, Nerd Fonts, ambiguous-width config
- [ ] `TextSource` impls beyond PTY: command output, file tail, plugin source
- [ ] Terminal profiles: default, big-text, ssh, logs, presentation, high-contrast
- [ ] Exit: interactive `bash` works inside a canvas node

## P4 — Projection System (README M4)

- [ ] Promote `ProjectionSurface` to a real `SurfaceType`; node holds source + projection
- [ ] Selectors: rows, columns, filter (substring/regex/levels), search, maxLines, follow
- [ ] Modes: live vs snapshot; extract vs highlight
- [ ] Presentation: wrap, reflow, line numbers, timestamps, match highlight, font scale, theme override
- [ ] Create/attach/detach projection from a node and from a selection
- [ ] Exit: terminal subranges pinned and arranged independently

## P5 — Lens & Zoom (README M5)

- [ ] Implement all `ZoomTarget` variants: workspace-fit, object, rectangle, terminal-range, projection, reading
- [ ] Animated transitions with easing (config `camera.easing`, `autoZoomAnimationMs`)
- [ ] Right-click autozoom; right-drag rectangle zoom; ctrl+right-click menu
- [ ] "Pin as View" materializes any zoom into an independent node (live or snapshot)
- [ ] Camera bookmarks as lens targets; named bookmarks UI
- [ ] Exit: any zoom can become a pinned view

## P6 — JS Host & Typed SDK (README M6–M7)

- [ ] `ScriptHost` trait; `V8ScriptHost` via `deno_core`/`v8`
- [ ] SWC transpile on load for `.ts/.mts/.js/.mjs` (no build step)
- [ ] Plugin lifecycle: manifest → `activate`/`deactivate`, disposables auto-cleanup
- [ ] Scoped permissions (terminal.read/write, fs paths, network origins, clipboard, process.spawn)
- [ ] Typed commands (input schema), typed throttled events, declarative widgets
- [ ] Schema-driven settings UI auto-generation
- [ ] Web-like globals (console, timers, structuredClone, crypto, URL, AbortController) — no DOM/Node
- [ ] Generate `.d.ts`: `fracterm`, `fracterm/config`, `fracterm/plugin`, `fracterm/widget`
- [ ] `QuickJsScriptHost` as optional fallback (trait keeps it swappable)
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