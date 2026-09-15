# Fracterm

**A spatial workspace of live text surfaces, projections, lenses, and commands.**

Fracterm is a Linux terminal emulator built around an infinite zoomable canvas.
Terminals, subrange views, zoom, widgets, reading mode, and dashboard
arrangement are not separate features — they are one unified system of nodes,
surfaces, projections, and lenses:

```text
Zooming      = temporary lens
Pinned view  = materialized lens
Reading mode = accessibility lens
Dashboard    = collection of lenses
```

Every action is a command. Plugins are typed TypeScript extensions. Terminals
are the primary live text source.

---

## Contents

- **Part I — Platform & Stack**: supported platforms, system dependencies,
  technology stack.
- **Part II — User Guide**: build, run, controls, configuration, profiles,
  layouts, plugins.
- **Part III — Functional Specification**: the complete design contract
  (§1–§29), including the milestone plan referenced by `TODO.md` (§26).
- **Part IV — Implementation Map**: how the design maps to the actual code,
  what is live vs. scaffolded vs. spec-only, design invariants, glossary.

**How to read this document.** Part III is the design contract — it describes
what Fracterm *is* and *will be*, stated in the affirmative. Part IV keeps
that honest: every subsystem is labeled **live** (implemented and verified),
**scaffold** (types/traits exist, behavior partial) or **spec** (design
only). Unmarked behavior in Part II is live; anything "(spec)" is not yet
implemented. `TODO.md` is the phased execution plan against this contract.

## The System at a Glance

```text
                    ┌─────────────────────────────────────┐
                    │             WORKSPACE               │
                    │   infinite zoomable canvas          │
                    │                                     │
                    │   CameraLens ◄──── zoom/pan/bookmarks│
                    │        │                            │
                    │   SceneGraph                       │
                    │        │                            │
                    │   ┌────┴─────┐  ┌─────────┐         │
                    │   │   NODE   │  │  NODE   │  ...    │
                    │   │ Transform│  │         │         │
                    │   │ Style    │  └─────────┘         │
                    │   │ Surface  │                      │
                    │   │ Projection                      │
                    │   │ InputBehavior                   │
                    │   │ PluginBehavior                  │
                    │   └────┬─────┘                      │
                    └────────┼────────────────────────────┘
                             │ renders via
              ContentPass ─ OverlayPass ─ PostProcessPass
                 (OpenGL 3.3+, capability-degraded)
                             ▲
        ┌───────────┬────────┴───────┬──────────────┐
   TerminalSurface  Projection   WidgetSurface   ReadingSurface
   (PTY + VT grid)  (live/       (display lists  (reflow lens)
                    snapshot,     from plugins)
                    selectors)

   Plugins: TypeScript → SWC → ScriptHost (QuickJS now, V8 spec'd)
            scoped permissions · typed commands/events · disposables
   Every action is a Command → keybindings, HUD, palette, tests, CLI
```

One abstraction explains every feature: a **terminal** is a node with a
`TerminalSurface`; a **subrange view** is a node whose surface is a
projection of another surface; **zooming** is a temporary lens, and **pinning**
materializes it; **reading mode** is an accessibility lens; a **dashboard**
is a collection of lenses. Nothing is a special case.

---

# Part I — Platform & Stack

## Supported Platform

Linux (X11 and Wayland via `winit`). macOS/Windows are not targets.

- **Display servers**: X11, Wayland.
- **Rendering**: OpenGL 3.3 Core Profile minimum (4.3 recommended).
  Capability-detected at startup; advanced effects degrade gracefully on
  weak drivers. No compute shaders required.
- **HiDPI**: integer scaling, fractional scaling where available,
  per-monitor DPI changes, crisp text at high DPI.
- **Directories**: XDG-first (`~/.config/fracterm`, `~/.local/share/fracterm`,
  `~/.cache/fracterm`), respecting `XDG_CONFIG_HOME`, `XDG_DATA_HOME`,
  `XDG_CACHE_HOME`.
- **Clipboard**: `clipboard` + primary selection, middle-click paste where
  appropriate, OSC 52 with permission.
- **Desktop integration**: `.desktop` file, AppStream metadata, window title
  updates, application ID. Optional packaging: AppImage, deb, rpm, Arch
  package, Flatpak where feasible.

## System Dependencies

- A Rust toolchain (stable, `cargo`).
- FreeType (font rasterization) and fontconfig (font discovery).
- clang/libclang (required by the `fontconfig` crate bindings).
- OpenGL 3.3+ capable GPU/driver.

Debian/Ubuntu:

```sh
sudo apt-get install -y build-essential libfreetype-dev libfontconfig1-dev libclang-dev clang
```

## Technology Stack

| Component | Choice | Reason |
|---|---|---|
| Core language | Rust | Performance, safety, plugin host stability |
| Windowing | `winit` | Linux X11/Wayland support |
| OpenGL bindings | `glow` | Direct OpenGL control |
| OpenGL context | `glutin` (+ `glutin-winit`) | Mature Linux OpenGL context creation |
| Terminal engine | `vte` parser + custom grid | Strong VT/ANSI compatibility |
| PTY | `portable-pty` | Native terminal spawning |
| Font discovery | fontconfig | Linux-native font selection |
| Font rasterization | FreeType (`freetype-rs`) | High-quality glyphs |
| Text shaping | HarfBuzz (planned) | Ligatures and complex shaping |
| Unicode | `unicode-width` | Terminal correctness |
| JavaScript engine | QuickJS (`rquickjs`) today; V8 (`deno_core`/`v8`) spec'd primary | High performance, modern JS |
| TypeScript support | SWC embedded transpilation | Direct `.ts` loading, no build step |
| Plugin isolation | Separate runtime per plugin | Security and stability |
| Serialization | `serde`, `serde_json` | API bridge and layouts |

**No Lua. No Python. JavaScript/TypeScript only.**

---

# Part II — User Guide

## Build & Run

```sh
cargo build --release
cargo run
```

`cargo run` opens a window on the current display. If no OpenGL 3.3 core
context is available the binary falls back to a headless logical loop (no
window; used for CI/testing).

## Startup Layout

The app opens with **one terminal**, centered and full-bleed: the grid is
aspect-fitted to the viewport from measured font metrics (e.g. 103×24 at
1280×720), framed by a centering camera fit. There are no demo nodes or
labels.

## Mouse & Camera Controls

| Input | Action |
|---|---|
| Mouse wheel | Zoom anchored at the cursor (smooth ~0.3 s easing) |
| Left-drag empty canvas | Pan |
| Middle-drag | Pan |
| Right-click an object | Autozoom to it |
| Right-drag | Rectangle zoom |
| Ctrl + right-click | Context menu (spec) |
| Alt + drag object | Move object (spec; drag-select via left currently) |
| `0` | Zoom to workspace fit |
| `f` | Smooth fly-to dashboard fit |
| `d` | Dashboard mode: tile 2-up from the margin + fit |

## Terminal Controls

| Input | Action |
|---|---|
| `Enter` or `i` | Focus terminal (keyboard → PTY) |
| `Esc` | Drop terminal focus back to workspace control |
| Typing (focused) | Sent to the shell |
| Arrows / named keys (focused) | Control sequences to the shell |
| Ctrl + key (focused) | ASCII control codes (e.g. Ctrl+C = 0x03) |
| Left drag (focused) | Select terminal text (spec) |
| Shift + wheel | Scroll terminal (spec) |
| Terminal app mouse mode | Forward mouse events to terminal (spec) |

## Terminal Node Management

| Input | Action |
|---|---|
| Click a terminal | Select it (accent border + resize handle) |
| Left-drag the node | Move it (edge snapping with guides) |
| Left-drag bottom-right corner | Resize it — grid + PTY follow (SIGWINCH) |
| Click empty canvas | Clear selection, start pan |
| `n` | Spawn a new terminal beside the focused one, wrapping below on narrow viewports (cap: 8) |

## Arrange Commands

| Key | Command |
|---|---|
| `h` | Tile horizontally |
| `v` | Tile vertically |
| `t` | Tile grid (3 columns) |
| `a` | Align left |
| `T`/`H`/`V`/`A` variants | Full set: tile, align (L/R/T/B), distribute (spec) |

## Projections & Bookmarks

| Input | Action |
|---|---|
| `p` | Pin the current viewport as a snapshot projection node |
| `b` | Save a camera bookmark (currently named `bm`) |
| `0`–`9` | Restore camera bookmark `bm<n>` |

Note: `b` saves under the name `bm` while digits restore `bm0`–`bm9`, so a
plain `b` save is not yet retrievable by digit — named bookmark UI is
planned (TODO P5).

Any zoom can be pinned: zoom into a terminal range, press `p`, and it becomes
an independent subrange-view node.

## Command Palette & HUD (spec)

- Auto-hiding HUD with configurable edge/hotkey.
- HUD actions: New Terminal, Command Palette, Save/Restore Layout,
  Toggle Effects, Plugin Console, Help.
- The **command palette** is the universal entry point; every action is a
  command (`terminal.new`, `camera.zoomToFit`, `layout.save`,
  `reading.enter`, …) bindable from keybindings, HUD, palette, tests, and CLI.

## Configuration

TypeScript-first. Default path:

```text
~/.config/fracterm/fracterm.config.ts
```

```ts
import { defineConfig } from "fracterm/config";

export default defineConfig({
  font: {
    family: "JetBrains Mono",
    size: 14,
    ligatures: true,
  },

  theme: {
    background: "#0b0d12",
    foreground: "#dfe3ee",
    cursor: "#82aaff",
    selection: "#2d3a55",
  },

  camera: {
    wheelZoomSpeed: 1.0,
    autoZoomAnimationMs: 250,
    easing: "cubic-out",
  },

  effects: {
    motionBlur: false,
    backgroundBlur: false,
  },

  input: {
    wheel: "zoom",
    rightClick: "autozoom",
    rightDrag: "rectangle-zoom",
    ctrlRightClick: "context-menu",
    altDrag: "move-object",
  },

  terminal: {
    scrollbackLines: 10000,
    copyOnSelect: false,
    ambiguousWidth: 1,
  },

  hud: {
    autoHide: true,
    edge: "top-left",
    hotkey: "Ctrl+Shift+H",
  },

  accessibility: {
    readingMode: {
      fontSize: 28,
      lineHeight: 1.6,
      highContrast: true,
      hideChrome: true,
    },
    cursor: { size: "large", color: "#ffffff" },
    reduceMotion: false,
  },

  profiles: {
    "big-text": {
      font: { size: 24, weight: 500 },
      theme: { background: "#000000", foreground: "#ffffff" },
    },
  },
});
```

## Terminal Profiles

Built-in profile presets:

```text
default  big-text  ssh  logs  presentation  high-contrast
```

## Plugins (Overview)

Plugins are TypeScript files loaded directly — SWC transpiles at load time,
no build step. A plugin registers commands, widgets, event listeners, and
schema-driven settings; permissions are scoped (e.g. `terminal.read:
created | granted | all`, `network: [origins]`). See Part III §9, §14–§17 for
the full SDK, and §21 for the widget system.

```ts
// fracterm.plugin.ts
import { definePlugin } from "fracterm/plugin";

export default definePlugin({
  id: "example.clock",
  name: "Clock",
  version: "0.1.0",
  api: "fracterm/1",
  permissions: ["workspace.read", "workspace.write", "widgets.render", "storage"],
  settings: {
    refreshMs: { type: "number", default: 1000, min: 100, max: 60000 },
  },
  activate(ctx) { /* ... */ },
  deactivate() { /* ... */ },
});
```

## Layouts & Persistence

Layouts serialize the whole scene graph — nodes, transforms, styles,
projections (selectors, modes), widget settings, groups, z-order, camera
bookmarks — not just terminal positions. Restores are exact and extensible.
See Part III §20.

## Troubleshooting / Diagnostics

- `fracterm doctor` (spec §24.4): checks OpenGL support, font availability,
  JS engine init, manifest validity, transpile errors, permission conflicts,
  layout schema version.
- Startup diagnostics log: font→path lines, first-PTY-bytes line.
- Known environment quirk: fish's first prompt can be slow (~14 s blank
  window) even with Primary-DA answered; typing once wakes it.

---

# Part III — Functional Specification

The complete design contract. The milestone plan in `TODO.md` maps to §26.

## 1. Core Design Philosophy

Fracterm is built around a small number of elegant primitives:

```text
Workspace
Node
Surface
Projection
Lens
Command
Event
Theme
Permission
```

Everything in Fracterm is expressible using these primitives.

### 1.1 Workspace

The infinite zoomable canvas. It contains nodes and a camera.

### 1.2 Node

A positioned object in the workspace.

Examples: terminal node, subrange view node, plugin widget node, group node,
reading pane node.

Nodes have components:

```text
Transform
Style
Focus
Input
Surface
Behavior
Permissions
```

### 1.3 Surface

A source of visual/textual content.

```text
TerminalSurface
TextViewSurface
WidgetSurface
GroupSurface
ReadingSurface
```

A terminal is not a special object. It is a node with a `TerminalSurface`.

### 1.4 Projection

A selected presentation of part of a surface.

```text
last 100 lines of a terminal
columns 0 through 120
lines matching ERROR
selected rectangle
frozen snapshot
reflowed reading view
```

A subrange view is not a special terminal feature. It is a projection of
another surface.

### 1.5 Lens

A way of viewing a surface or projection.

- The camera is a lens onto the workspace.
- Zooming into a terminal rectangle is a lens onto a terminal surface.
- A pinned subrange view is a node whose surface is a projection.
- Reading mode is a lens with accessibility presentation rules.

This makes zooming and subrange views conceptually identical:

```text
Zooming = temporary lens
Pinned view = materialized lens
Reading mode = accessibility lens
Dashboard = collection of lenses
```

---

## 2. Architecture

```text
Workspace
  CameraLens
  SceneGraph
    Node
      Transform
      Style
      Surface
      Projection
      InputBehavior
      PluginBehavior
```

Improves: flexibility, plugin extensibility, future features, code reuse,
UI consistency.

---

## 3. Technology Stack

See Part I — Technology Stack.

---

## 4. Rendering Design

The renderer must remain OpenGL-required but structured, not a monolith.

### 4.1 Render graph

```text
ContentPass
  terminal layers
  projection layers
  widget layers

OverlayPass
  borders
  handles
  selection
  HUD

PostProcessPass
  motion blur
  background blur
  optional effects
```

### 4.2 OpenGL compatibility

Minimum target: **OpenGL 3.3 Core Profile**. Recommended: **OpenGL 4.3 Core
Profile**.

Rules:

- Do not require compute shaders.
- Use framebuffer objects for effects.
- Detect driver capabilities at startup.
- Disable advanced effects gracefully on weak drivers.
- Provide a software HUD fallback if necessary, but canvas rendering may
  require OpenGL.

### 4.3 Text rendering (hybrid zoom strategy)

- **Dashboard zoom**: cached terminal layer textures; update only damaged
  regions.
- **Near zoom**: render glyphs directly at the target screen size — avoid
  blurry texture scaling.
- **Very large zoom**: rasterize glyphs at high pixel sizes or use
  high-quality SDF with dynamic fallback.

This is essential for near-sightedness support.

### 4.4 Glyph cache

Glyph cache keys include:

```text
font id
glyph id
pixel size
subpixel position bucket
style flags
color mode
```

This gives sharp text and efficient reuse.

---

## 5. Input Model

Explicit, soft (not rigid) interaction modes.

### 5.1 Workspace Mode

Default canvas navigation.

| Input | Action |
|---|---|
| Mouse wheel | Zoom workspace |
| Left drag empty canvas | Pan |
| Middle drag | Pan |
| Right click | Autozoom to object |
| Right drag | Rectangle zoom |
| Ctrl + right click | Context menu |
| Alt + drag object | Move object |

### 5.2 Terminal Mode

Activated when a terminal is focused.

| Input | Action |
|---|---|
| Keyboard | Sent to terminal |
| Left drag | Select terminal text |
| Shift + wheel | Scroll terminal |
| Wheel | Zoom workspace by default |
| Terminal app mouse mode | Forward mouse events to terminal |
| Alt + drag | Move terminal instead |

### 5.3 Reading Mode

A zoomed, accessibility-focused view:

- large text,
- high contrast,
- optional reflow,
- hidden HUD,
- hidden borders,
- line focus,
- adjustable line spacing,
- optional dimming of non-focused lines.

### 5.4 Dashboard Mode

Arrangement mode for views and widgets:

- snapping,
- alignment guides,
- grid display,
- object grouping,
- keyboard nudging,
- tiling commands.

---

## 6. Zoom Model

Zoom is a transition between lenses, not merely a camera action.

### 6.1 Zoom targets

```ts
type ZoomTarget =
  | { type: "workspace-fit" }
  | { type: "object"; nodeId: string }
  | { type: "rectangle"; rect: Rect }
  | { type: "terminal-range"; terminalId: string; range: GridRange }
  | { type: "projection"; projectionId: string }
  | { type: "reading"; sourceId: string; range?: GridRange };
```

### 6.2 Zoom behavior

```text
Right-click object              -> zoom to object
Right-click terminal selection  -> zoom to selected grid range
Right-drag rectangle            -> zoom to rectangle
Ctrl + right-click              -> open context menu
```

### 6.3 Pinning a zoom

Any zoom target can be pinned:

```text
Zoomed terminal range
  -> user presses "Pin as View"
  -> becomes independent SubrangeView node
```

This makes zooming and dashboard creation feel unified.

---

## 7. Terminal Model

The terminal is a live text source.

### 7.1 TextSource abstraction

```text
TextSource
  Terminal PTY source
  Command output source
  File tail source
  Plugin-provided text source
```

This allows future flexibility without redesign. A terminal is the first and
most important implementation.

### 7.2 Terminal compatibility requirements

Required:

```text
ANSI escape sequences
VT100/xterm compatibility
8-bit color
16-bit color
24-bit true color
scrollback
alternate screen
cursor styles
mouse tracking
bracketed paste
OSC title updates
safe OSC clipboard support
line wrapping
resize events
Unicode wide characters
grapheme-aware rendering
```

Recommended:

```text
OSC 8 hyperlinks
synchronized output
focus events
kitty keyboard protocol if feasible
emoji support
Nerd Font glyphs
ambiguous width configuration
```

---

## 8. Subrange Views (Projection System)

A subrange view is a `ProjectionSurface`.

### 8.1 ProjectionSurface

```ts
interface ProjectionSurface {
  source: SurfaceId;
  selector: ProjectionSelector;
  mode: "live" | "snapshot";
  presentation: ProjectionPresentation;
}
```

### 8.2 Projection selector

```ts
interface ProjectionSelector {
  rows?: RowRange;
  columns?: ColumnRange;
  filter?: FilterSpec;
  search?: string;
  maxLines?: number;
  follow?: boolean;
}
```

### 8.3 Filter specification

```ts
interface FilterSpec {
  type: "substring" | "regex" | "levels";
  pattern?: string;
  levels?: Array<"TRACE" | "DEBUG" | "INFO" | "WARN" | "ERROR" | "FATAL">;
  mode: "extract" | "highlight";
  caseSensitive?: boolean;
}
```

This makes log dashboards much more ergonomic.

### 8.4 Presentation options

```ts
interface ProjectionPresentation {
  wrap?: boolean;
  reflow?: boolean;
  showLineNumbers?: boolean;
  showSourceTimestamp?: boolean;
  highlightMatches?: boolean;
  fontSizeScale?: number;
  themeOverride?: ThemeOverride;
}
```

A projection can be used as: dashboard tile, log monitor, reading pane,
search result view, accessibility magnifier.

---

## 9. TypeScript SDK

### 9.1 Direct TypeScript loading

Fracterm loads `.ts`, `.mts`, `.js`, `.mjs` directly — no mandatory build
step. SWC transpiles TypeScript to JavaScript at load time. Type checking
remains a development-time concern.

### 9.2 Plugin manifest in TypeScript

```ts
// fracterm.plugin.ts
import { definePlugin } from "fracterm/plugin";

export default definePlugin({
  id: "example.clock",
  name: "Clock",
  version: "0.1.0",
  api: "fracterm/1",

  permissions: [
    "workspace.read",
    "workspace.write",
    "ui.menus",
    "ui.keybindings",
    "widgets.render",
    "storage",
  ],

  settings: {
    refreshMs: {
      type: "number",
      default: 1000,
      min: 100,
      max: 60000,
      description: "Clock refresh interval",
    },
  },

  activate(ctx) {
    // plugin code
  },

  deactivate() {
    // cleanup
  },
});
```

JSON manifests are still supported for simple cases; TypeScript manifests are
more ergonomic.

### 9.3 Typed commands

```ts
ctx.commands.register({
  id: "terminal.new",
  title: "New Terminal",
  category: "Terminal",

  input: {
    command: { type: "string", optional: true },
    cwd: { type: "string", optional: true },
    title: { type: "string", optional: true },
  },

  async run(input) {
    const terminal = await ctx.terminals.spawn({
      command: input.command,
      cwd: input.cwd,
      title: input.title,
    });

    await ctx.camera.zoomTo({
      type: "object",
      nodeId: terminal.id,
    });

    return { terminalId: terminal.id };
  },
});
```

Benefits: command palette integration, keybinding integration, plugin
integration, typed parameters, easier testing, easier documentation.

### 9.4 Typed events

```ts
ctx.events.on("workspace:objectCreated", event => {
  ctx.log.info("Created object", event.nodeId);
});

ctx.events.on("terminal:output", event => {
  // throttled, permission-checked
});
```

Terminal output events must be throttled and permission-gated. Plugins should
not receive raw high-throughput terminal output unless explicitly permitted
and rate-limited.

### 9.5 Declarative widget API

```ts
import { defineWidget } from "fracterm/widget";

export const clock = defineWidget({
  id: "example.clock",
  title: "Clock",

  defaultSize: {
    width: 320,
    height: 140,
  },

  state: () => ({
    now: Date.now(),
  }),

  timers: [
    {
      everyMs: 1000,
      run(state) {
        state.now = Date.now();
      },
    },
  ],

  view(state, ui) {
    ui.clear("#101018");

    ui.text(new Date(state.now).toLocaleTimeString(), {
      x: 16,
      y: 70,
      scale: 2.5,
      color: "#e8e8f0",
    });
  },
});
```

An advanced immediate-mode escape hatch remains:

```ts
drawAdvanced(ctx) {
  // custom immediate-mode display list
}
```

### 9.6 Settings UI generation

If a plugin defines settings with schemas, Fracterm automatically generates
the options UI:

```ts
settings: {
  refreshMs: { type: "number", default: 1000, min: 100, max: 10000 },
  timezone: { type: "string", default: "local" },
  showSeconds: { type: "boolean", default: true },
}
```

The border options menu then shows:

```text
Refresh: [1000]
Timezone: [local]
Show seconds: [x]
```

This makes plugins feel native.

---

## 10. Configuration

TypeScript-first (full example in Part II). Default path:

```text
~/.config/fracterm/fracterm.config.ts
```

Terminal profiles are a key ergonomic feature:

```text
default
big-text
ssh
logs
presentation
high-contrast
```

---

## 11. Object Arrangement

### 11.1 Groups

```text
Group
  Terminal
  SubrangeView
  Widget
```

A group can be: moved, zoomed, saved as a layout fragment, collapsed,
aligned, distributed.

### 11.2 Snapping

Optional snapping features:

```text
grid snapping
edge snapping
alignment guides
equal spacing
object distribution
```

### 11.3 Arrange commands

```text
Arrange Tile Horizontally
Arrange Tile Vertically
Arrange Grid
Arrange Cascade
Align Left
Align Right
Align Top
Align Bottom
Distribute Horizontally
Distribute Vertically
Zoom to Group
```

### 11.4 Camera bookmarks

Users save camera positions:

```text
Overview
Logs
Build
Monitoring
Reading
```

A camera bookmark is just another lens target.

---

## 12. Border and HUD Design

### 12.1 Border controls

Visible on hover/select:

```text
[x] close
[⋯] options
resize handles
drag handle/title bar
```

The options popover includes:

```text
Title
Opacity
Style
Font scale
Input mode
Background blur
Border color
Always on top
Save as default
```

For terminals:

```text
Profile
Scrollback
Reflow
Application mouse indicator
Copy on select
```

For projections:

```text
Live / Snapshot
Follow source
Edit filter
Detach
Reading mode
```

For widgets:

```text
Plugin settings
Update interval
Widget-specific actions
```

### 12.2 HUD

The HUD remains auto-hiding. HUD actions:

```text
New Terminal
Command Palette
Save Layout
Restore Layout
Toggle Effects
Plugin Console
Help
```

The command palette is the universal entry point. Every action is available
from the palette — more elegant than burying features in menus.

---

## 13. JavaScript Runtime

V8 is the primary engine. A small Rust-side abstraction keeps it swappable:

```rust
trait ScriptHost {
    fn load_plugin(&mut self, manifest: PluginManifest) -> Result<PluginId>;
    fn unload_plugin(&mut self, id: PluginId) -> Result<()>;
    fn call(&mut self, id: PluginId, method: &str, args: Value) -> Result<Value>;
    fn dispatch_event(&mut self, event: Event) -> Result<()>;
}
```

Primary implementation: `V8ScriptHost`. Optional future implementation:
`QuickJsScriptHost`. The public plugin API remains JavaScript/TypeScript.
This improves long-term compatibility without burdening plugin authors.

*Current implementation note:* `QuickJsScriptHost` (via `rquickjs`) is live
behind the `ScriptHost` trait; a `V8Host` stub exists.

---

## 14. Plugin Environment (Web Compatibility)

Standard Web-like globals where safe:

```text
console
setTimeout
setInterval
clearTimeout
clearInterval
queueMicrotask
structuredClone
TextEncoder
TextDecoder
AbortController
URL
URLSearchParams
crypto.randomUUID
```

Never provided:

```text
DOM
Node.js built-ins
raw process APIs
raw filesystem APIs
raw OpenGL APIs
```

Host-mediated, permission-gated APIs:

```text
fetch
storage
clipboard
terminal read/write
filesystem sandbox
process spawning
```

Good ergonomics without giving up security.

---

## 15. Permissions

Scoped, not just boolean:

```ts
permissions: [
  "workspace.read",
  "workspace.write",
  "terminal.create",
  {
    permission: "terminal.read",
    scope: "created",
  },
  {
    permission: "storage",
    scope: "plugin",
  },
  {
    permission: "network",
    scope: ["https://api.example.com/*"],
  },
]
```

Possible scopes:

```text
terminal.read: created | granted | all
terminal.write: created | granted | all
fs.read: specific paths
fs.write: specific paths
network: specific origins
clipboard: read | write
process.spawn: allowed command patterns
```

---

## 16. Plugin Lifecycle

```ts
export default definePlugin({
  id: "example.dashboard",
  name: "Example Dashboard",
  version: "0.1.0",

  async activate(ctx) {
    ctx.log.info("Activating plugin");
  },

  async deactivate(ctx) {
    ctx.log.info("Deactivating plugin");
  },
});
```

All registrations return disposables:

```ts
const disposable = ctx.commands.register(...);

ctx.onDeactivate(() => {
  disposable.dispose();
});
```

The host also automatically disposes plugin resources on unload.

---

## 17. Command System

Every action is a command:

```text
terminal.new
terminal.close
view.pinSelection
view.snapshot
camera.zoomToFit
camera.zoomToSelection
camera.reset
layout.save
layout.restore
theme.toggle
effects.toggleMotionBlur
hud.toggle
palette.open
reading.enter
```

Benefits:

- keybindings bind to commands,
- plugins invoke commands,
- HUD uses commands,
- command palette uses commands,
- tests invoke commands,
- future CLI invokes commands.

---

## 18. Accessibility

Near-sightedness zoom becomes a first-class accessibility feature.

### 18.1 Reading lens

A reading lens can reflow text instead of merely magnifying terminal cells:

```text
larger font
increased line height
high-contrast theme
hidden chrome
line focus
optional ruler
optional word spacing adjustment
optional font weight increase
```

### 18.2 Quick reading action

Context menu:

```text
Read selection
Read current line
Read last 50 lines
Read filtered selection
```

### 18.3 Accessibility options

```ts
accessibility: {
  readingMode: {
    fontSize: 28,
    lineHeight: 1.6,
    highContrast: true,
    hideChrome: true,
  },

  cursor: {
    size: "large",
    color: "#ffffff",
  },

  reduceMotion: false,
}
```

---

## 19. Linux Compatibility

Fracterm should feel native on Linux.

### 19.1 XDG directories

```text
~/.config/fracterm
~/.local/share/fracterm
~/.cache/fracterm
```

Respect:

```text
XDG_CONFIG_HOME
XDG_DATA_HOME
XDG_CACHE_HOME
```

### 19.2 Clipboard

```text
clipboard
primary selection
middle-click paste where appropriate
OSC 52 with permission
```

### 19.3 HiDPI

```text
integer scaling
fractional scaling where available
per-monitor DPI changes
crisp text at high DPI
```

### 19.4 Desktop integration

```text
.desktop file
AppStream metadata
window title updates
application ID
```

Optional packaging:

```text
AppImage
deb
rpm
Arch package
Flatpak where feasible
```

---

## 20. Persistence

Layouts store the scene graph, not just terminal positions. A layout
includes:

```text
camera bookmarks
nodes
transforms
styles
terminal profiles
projection selectors
widget settings
group membership
z-order
saved modes
```

Example:

```json
{
  "version": 1,
  "camera": {
    "x": 0,
    "y": 0,
    "zoom": 1
  },
  "bookmarks": [],
  "nodes": [],
  "groups": []
}
```

---

## 21. Plugin Widget System

Widgets participate in workspace life without being terminals.

Widget capabilities:

```text
render 2D content
receive pointer events
receive keyboard focus if requested
expose settings
register commands
subscribe to events
save local state
```

Widgets never directly access OpenGL — they produce display lists. This keeps
plugins safe and the renderer fast.

---

## 22. Search

Built into terminals and projections:

```text
incremental search
regex search
case sensitivity toggle
match highlighting
jump to previous/next match
create projection from search results
pin search as view
```

Search is highly synergistic with subrange views.

---

## 23. Context Menu

Do not overload right-click. Use:

```text
Right-click
  autozoom

Ctrl + right-click
  context menu

Long-press right-click
  context menu
```

Context menu items depend on target.

For terminal selection:

```text
Copy
Pin as Live View
Pin as Snapshot
Zoom to Selection
Read Selection
Filter Selection
Search Selection
```

For terminal object:

```text
New Terminal From Profile
Rename
Opacity
Style
Input Mode
Save to Layout
Close
```

For projection:

```text
Follow Source
Edit Filter
Snapshot
Detach
Reading Mode
Close
```

This keeps right-click zoom intact while preserving discoverability.

---

## 24. Developer Tooling

Fracterm should be pleasant for plugin developers.

### 24.1 Generated types

Generate TypeScript definitions from the Rust API:

```text
fracterm.d.ts
fracterm/config.d.ts
fracterm/plugin.d.ts
fracterm/widget.d.ts
```

This prevents documentation drift.

### 24.2 In-app plugin console

The plugin console shows:

```text
console.log
console.warn
console.error
plugin load state
permission grants
runtime exceptions
source-mapped stack traces
```

### 24.3 Hot reload

Development mode watches plugin files:

```text
edit main.ts
  -> SWC transpile
  -> deactivate plugin
  -> reload plugin
  -> restore workspace state where possible
```

### 24.4 Plugin doctor

```text
fracterm doctor
```

Checks:

```text
OpenGL support
font availability
JS engine initialization
plugin manifest validity
TypeScript transpilation errors
permission conflicts
layout schema version
```

---

## 25. Testing Strategy

### 25.1 Terminal tests

```text
vttest-style conformance tests
Unicode width tests
grapheme tests
wide character tests
emoji tests
escape sequence tests
resize tests
alternate screen tests
mouse mode tests
bracketed paste tests
```

### 25.2 Rendering tests

Golden-image tests for:

```text
terminal rendering
zoom precision
glyph crispness
subrange views
HUD rendering
theme changes
opacity changes
motion blur toggling
```

### 25.3 Plugin API tests

TypeScript plugin contract tests:

```text
plugin loads
command registers
widget renders
permissions enforce
disposables clean up
hot reload works
errors are reported
```

---

## 26. Milestones

The implementation order. `TODO.md` tracks execution against these.

### Milestone 1: Core canvas primitives

Build: workspace, camera, node, transform, style, OpenGL renderer.

Exit: rectangles and text can be zoomed and panned smoothly.

### Milestone 2: Text surface rendering

Build: glyph atlas, text batching, sharp zoom strategy.

Exit: text remains crisp during deep zoom.

### Milestone 3: Terminal surface

Build: PTY, terminal parser, grid, damage tracking, keyboard input.

Exit: interactive bash works inside a canvas node.

### Milestone 4: Projection system

Build: surface projections, row/column selectors, snapshot/live modes,
filters.

Exit: terminal subrange views can be pinned and arranged.

### Milestone 5: Lens zooming

Build: zoom to object, zoom to rectangle, zoom to terminal range, reading
lens, camera bookmarks.

Exit: any zoom can become a pinned view.

### Milestone 6: JavaScript host

Build: V8 isolate, module loader, SWC TypeScript transpile, plugin lifecycle,
permissions, console.

Exit: a TypeScript plugin can register commands and widgets.

### Milestone 7: Typed SDK

Build: defineConfig, definePlugin, defineWidget, typed commands, typed
events, generated d.ts.

Exit: pleasant TypeScript development experience.

### Milestone 8: Dashboard ergonomics

Build: groups, snapping, alignment, arrange commands, layout save/restore.

Exit: users can build dashboards quickly.

### Milestone 9: Accessibility and polish

Build: reading mode, high contrast themes, reduced motion, large cursor,
font profiles.

Exit: near-sighted reading workflow is excellent.

---

## 27. What Specifically Becomes Better?

### More ergonomic

TypeScript-first SDK, declarative widgets, typed commands, generated types,
hot reload, command palette, settings schema UI, reading mode, context-aware
menus.

### More compatible

Stronger terminal compatibility, Unicode-aware rendering,
fontconfig/HarfBuzz integration, X11/Wayland support, HiDPI support, OpenGL
capability detection, stable plugin API versioning, optional engine
abstraction.

### More flexible

Surfaces and projections, any text source can become a view, projections can
be live or snapshot, projections can be filtered, any zoom can be pinned,
groups and layouts, camera bookmarks, terminal profiles, plugin widgets with
settings.

### More elegant

Fewer special cases, unified lens model, command-driven architecture,
capability-based permissions, display-list plugin rendering, scene graph
instead of ad-hoc object types, consistent lifecycle and disposal.

---

## 28. Final Recommended Stack

```text
Core:
  Rust

Graphics:
  OpenGL via glow + glutin
  render-graph architecture
  dynamic high-quality glyph rendering

Terminal:
  Rust terminal engine
  Linux PTY
  strong VT/Unicode compatibility

Workspace:
  scene graph
  node/component model
  surfaces
  projections
  lenses
  commands
  events

Scripting:
  V8 JavaScript engine
  TypeScript loaded directly via SWC
  no Lua
  no Python

Plugin SDK:
  TypeScript-first
  typed commands
  typed events
  declarative widgets
  schema-driven settings
  generated d.ts files

Security:
  isolated plugin execution
  scoped permissions
  display-list rendering only
  host-mediated privileged operations
```

---

## 29. Summary

Fracterm is not merely:

```text
a terminal emulator with zoom
```

It is:

```text
a spatial lens system for live text surfaces
```

where terminals are the primary live text source, subrange views are
projections, zooming is lens navigation, plugins are typed TypeScript
extensions, and every action is a command.

---

# Part IV — Implementation Map

How the design contract maps to the actual code, what stands today, and the
invariants that keep it correct. Status labels: **live** (implemented,
verified — display or test), **scaffold** (types/traits exist, behavior
partial), **spec** (design only; see TODO.md phase for landing it).

## Source Module Map

| Module | Role | Spec § | Status |
|---|---|---|---|
| `src/lib.rs` | Crate root; ID types (`NodeId`, `SurfaceId`, `CommandId`, `EventId`); re-exports | §1 | live |
| `src/workspace.rs` | `Workspace` + `SceneGraph`; node/component storage, z-order, groups | §1.1–1.2, §2 | live |
| `src/node.rs` | `Node` + components: `Transform`, `Style`, `Surface`, `Projection`, `InputBehavior`, `PluginBehavior` | §1.2 | live |
| `src/transform.rs` | Position/size math | §1.2 | live |
| `src/surface.rs` | `Surface`, `SurfaceType`, `Cell`, `Color` — textual content sources | §1.3 | live |
| `src/terminal.rs` | `Terminal`, `TerminalGrid`, `TextSource` trait + PTY/command/file-tail sources | §7 | live (sources scaffold) |
| `src/vt.rs` | VT/ANSI/xterm parser (`vte`): SGR, cursor, erase, alt screen, 256/24-bit color | §7.2 | live |
| `src/pty.rs` | `PtySession` via `portable-pty`: spawn, reader thread, write, resize (SIGWINCH) | §7 | live |
| `src/projection.rs` | `ProjectionSurface`, selectors (rows/cols/filter/search/tail/follow), live/snapshot modes | §8 | live (presentation scaffold) |
| `src/camera.rs` | `Camera`: eased pan/zoom, cursor-anchored wheel zoom, bookmarks, `fit_rect` | §6 | live |
| `src/lens.rs` | `CameraLens`, `Lens`, `ZoomTarget`, `Rect`, `GridRange` | §1.5, §6 | live (targets partial) |
| `src/canvas.rs` | GL context/capabilities, `RectRenderer` batching, `RenderTarget` FBOs, `RenderGraphExecutor` | §4.1–4.2 | live (PostProcess placeholder) |
| `src/text.rs` | fontconfig discovery, FreeType rasterization, `Atlas`/`GlyphKey` cache, `TextRenderer`, zoom-size strategy | §4.3–4.4 | live (far-zoom layers pending) |
| `src/rendering.rs` | Logical `RenderGraph` model (`RenderPass`, node config, glyph-atlas stub) | §4.1 | live |
| `src/window.rs` | winit event loop + glutin surface; terminal sessions, drag state machine, keyboard encoding, draw frame | §5 | live |
| `src/arrange.rs` | Pure layout math: tile/align/distribute/cascade, snapping, `place_beside`, grid metrics | §11 | live |
| `src/app.rs` | `App`, `AppState`, `InteractionMode`, `CliArgs`; headless fallback loop | §5.1 | scaffold (loop is a stub) |
| `src/config.rs` | `Config` tree: font/theme/camera/effects/input/terminal/hud/accessibility/profiles | §10 | scaffold (TS loading pending) |
| `src/theme.rs` | `Theme` colors | §1 | live |
| `src/event.rs` | `Event`, `EventBus` (serde, throttling) | §9.4 | live |
| `src/command.rs` | `Command`, typed `CommandInput` params | §17 | scaffold |
| `src/permission.rs` | `Permission`, `PermissionScope`, `PermissionContext` | §15 | live |
| `src/plugin.rs` | `Plugin` trait, `Widget`, `V8Host` stub, `PluginSDK` | §21 | scaffold |
| `src/script.rs` | `ScriptHost` trait, `PluginManifest`, `QuickJsScriptHost` (rquickjs), SWC `transpile_ts` | §13, §9.1 | live (QuickJS; V8 spec'd) |

Pipeline: `window.rs` pumps winit events → mutates `Workspace`/`Camera`
→ drains PTY bytes through `vt.rs` into `Terminal` grids → `canvas.rs` +
`text.rs` rasterize/batch a frame → FBO passes composite to screen.
Plugins enter via `script.rs` behind `ScriptHost`.

## Implementation Status by Subsystem

| Subsystem | Status | Notes |
|---|---|---|
| Canvas, camera, pan/zoom | **live** | eased, cursor-anchored, bookmarked; drift-free |
| Text rendering | **live** | atlas + subpixel + near/large zoom sizes; far-zoom layer textures pending |
| Terminal (PTY/VT/grid) | **live** | SGR 16/256/24-bit, alt screen, scrollback, keyboard; mouse/paste/OSC pending |
| Projections | **live** | selectors + live/snapshot; presentation options pending |
| Lens zoom + pin | **live** | workspace-fit/object/rect targets; terminal-range/reading targets spec |
| Script host + TS load | **live** | QuickJS + SWC; V8 behind trait pending |
| Permissions | **live** | scoped matching; enforcement surface partial |
| Arrange/dashboard | **live** | tile/align/distribute/snap/persist; guides UI + multi-select drag pending |
| Config system | **scaffold** | Rust `Config` tree + profiles; TS config file loading pending |
| Command system | **scaffold** | typed params exist; palette/keybinding wiring pending |
| HUD / palette / menus | **spec** | — |
| Reading mode / accessibility | **spec** | config struct exists |
| Widgets (display lists) | **scaffold** | `Widget` type exists; render pipeline spec |
| Search / context menus | **spec** | — |
| Packaging / XDG / CLI | **spec** | `CliArgs` parsed-but-unused |

## Design Invariants

The non-negotiables any change must preserve (full regression list in
`TODO.md` → Development Guardrails):

1. **Camera contract**: direct manipulation sets current + target +
   `animating = false`; animated moves set target + `animating = true`.
   Anything else re-introduces "the camera steers itself".
2. **IDs have one home**: `NodeId`/`SurfaceId`/`CommandId`/`EventId` live in
   `src/lib.rs`; no private re-exports.
3. **Glyph geometry is measured, never assumed**: cell metrics from the `M`
   glyph advance; VAO byte offsets fixed; atlas keys include the supplying
   face.
4. **PTY writer is singular**: one held writer; `take_writer` fails after
   first call.
5. **VT must answer Primary DA** or shells (fish) stall their first prompt.
6. **Plugins render by display list**, never direct OpenGL; privileged ops
   are host-mediated and permission-scoped.
7. **Every action is a command** — new features register commands, not
   ad-hoc key handlers.

## Glossary

| Term | Meaning |
|---|---|
| **Workspace** | The infinite zoomable canvas holding nodes + camera |
| **Node** | A positioned object; owns components (Transform, Style, Surface, Projection, Input, Behaviors) |
| **Surface** | A source of visual/textual content (terminal, text view, widget, reading) |
| **Projection** | A selected presentation of part of a surface (rows, columns, filter, search, tail; live or snapshot) |
| **Lens** | A way of viewing a surface/workspace; the camera is the workspace lens; zoom = temporary lens; pin = materialized lens |
| **Camera bookmark** | A saved lens target, restorable by digit |
| **Command** | The universal unit of action; bindable from keys, HUD, palette, plugins, tests, CLI |
| **TextSource** | Anything that feeds a surface: PTY, command output, file tail, plugin |
| **Display list** | The plugin-safe rendering output (no direct OpenGL) |
| **Permission scope** | Parameterized grant, e.g. `terminal.read: created`, `network: [origins]` |
| **Profile** | A named font/theme/terminal preset (`big-text`, `ssh`, `logs`, …) |
| **Layout** | Persisted scene graph: nodes, transforms, projections, groups, z-order, bookmarks |
