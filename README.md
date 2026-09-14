Yes. The previous specification is already implementable, but it can be made **more ergonomic, more compatible, more flexible, and more elegant** by changing a few foundational design decisions.

The biggest improvement is to stop treating these as separate features:

- terminals,
- subrange views,
- zoom,
- widgets,
- reading mode,
- dashboard arrangement.

Instead, model them as one unified system:

> **Fracterm is a spatial workspace of live text surfaces, projections, lenses, and commands.**

That single abstraction makes the whole design cleaner and more powerful.

---

# Fracterm v2 — Improved Specification

## 1. Core Design Philosophy

Fracterm should be built around a small number of elegant primitives:

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

Everything in Fracterm should be expressible using these primitives.

### 1.1 Workspace

The infinite zoomable canvas.

It contains nodes and a camera.

### 1.2 Node

A positioned object in the workspace.

Examples:

- terminal node,
- subrange view node,
- plugin widget node,
- group node,
- reading pane node.

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

Examples:

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

Examples:

```text
last 100 lines of a terminal
columns 0 through 120
lines matching ERROR
selected rectangle
frozen snapshot
reflowed reading view
```

A subrange view is not a special terminal feature. It is a projection of another surface.

### 1.5 Lens

A way of viewing a surface or projection.

The camera is a lens onto the workspace.

Zooming into a terminal rectangle is a lens onto a terminal surface.

A pinned subrange view is a node whose surface is a projection.

Reading mode is a lens with accessibility presentation rules.

This makes zooming and subrange views conceptually identical:

```text
Zooming = temporary lens
Pinned view = materialized lens
Reading mode = accessibility lens
Dashboard = collection of lenses
```

This is more elegant and more flexible than treating zoom, views, and terminals as separate subsystems.

---

## 2. Improved Architecture

### Previous design

```text
Terminal
SubrangeView
Widget
Camera
```

### Improved design

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

This improves:

- flexibility,
- plugin extensibility,
- future features,
- code reuse,
- UI consistency.

---

## 3. Improved Technology Stack

Keep the core stack, but refine it for compatibility and elegance.

| Component | Recommended Choice | Reason |
|---|---|---|
| Core language | Rust | Performance, safety, plugin host stability |
| Windowing | `winit` | Linux X11/Wayland support |
| OpenGL bindings | `glow` | Direct OpenGL control |
| OpenGL context | `glutin` | Mature Linux OpenGL context creation |
| Terminal engine | `alacritty_terminal` or `wezterm-term` style engine | Strong VT compatibility |
| PTY | Linux PTY via `portable-pty` or direct PTY | Native terminal spawning |
| Font discovery | fontconfig | Linux-native font selection |
| Font rasterization | FreeType | High-quality glyphs |
| Text shaping | HarfBuzz | Ligatures and complex shaping |
| Unicode | Unicode segmentation, width, grapheme handling | Terminal correctness |
| UI overlay | `egui` or custom immediate-mode UI | HUD, menus, inspectors |
| JavaScript engine | V8 | High performance, modern JS |
| JS embedding | `deno_core` or a thin `v8` crate wrapper | Rust integration |
| TypeScript support | SWC embedded transpilation | Direct `.ts` loading |
| Plugin isolation | V8 isolates / realms | Security and stability |
| Async runtime | `tokio` | PTY and plugin async operations |
| Serialization | `serde`, `serde_json` | API bridge and layouts |

No Lua.

No Python.

JavaScript/TypeScript only.

---

## 4. More Compatible Rendering Design

The original spec says OpenGL. That should remain required, but the renderer should be structured so it does not become a monolith.

### 4.1 Render graph

Use a small render-graph architecture:

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

This is cleaner than ad-hoc draw calls.

### 4.2 OpenGL compatibility

Minimum target:

```text
OpenGL 3.3 Core Profile
```

Recommended:

```text
OpenGL 4.3 Core Profile
```

Rules:

- Do not require compute shaders.
- Use framebuffer objects for effects.
- Detect driver capabilities at startup.
- Disable advanced effects gracefully on weak drivers.
- Provide a software HUD fallback if necessary, but canvas rendering may require OpenGL.

### 4.3 Text rendering improvements

For elegance and sharpness, use a hybrid text renderer:

#### Dashboard zoom

Use cached terminal layer textures.

Update only damaged regions.

#### Near zoom

Render glyphs directly at the target screen size.

Avoid blurry texture scaling.

#### Very large zoom

Rasterize glyphs at high pixel sizes or use high-quality SDF with dynamic fallback.

This is essential for near-sightedness support.

### 4.4 Glyph cache

Glyph cache keys should include:

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

## 5. More Ergonomic Input Model

The original input model works, but it can be made more elegant by introducing explicit interaction modes.

## 5.1 Interaction modes

Fracterm should have four primary interaction modes:

```text
Workspace Mode
Terminal Mode
Reading Mode
Dashboard Mode
```

These modes should be soft, not rigid.

### Workspace Mode

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

### Terminal Mode

Activated when a terminal is focused.

| Input | Action |
|---|---|
| Keyboard | Sent to terminal |
| Left drag | Select terminal text |
| Shift + wheel | Scroll terminal |
| Wheel | Zoom workspace by default |
| Terminal app mouse mode | Forward mouse events to terminal |
| Alt + drag | Move terminal instead |

### Reading Mode

A zoomed, accessibility-focused view.

Features:

- large text,
- high contrast,
- optional reflow,
- hidden HUD,
- hidden borders,
- line focus,
- adjustable line spacing,
- optional dimming of non-focused lines.

### Dashboard Mode

Arrangement mode for views and widgets.

Features:

- snapping,
- alignment guides,
- grid display,
- object grouping,
- keyboard nudging,
- tiling commands.

This makes the UX more predictable and more ergonomic.

---

## 6. More Elegant Zoom Model

Zoom should not be implemented as a special camera action only.

Zoom should be a transition between lenses.

### 6.1 Zoom targets

A zoom target can be:

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

Right-click:

```text
Right-click object
  -> zoom to object

Right-click terminal selection
  -> zoom to selected grid range

Right-drag rectangle
  -> zoom to rectangle

Ctrl + right-click
  -> open context menu
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

## 7. More Flexible Terminal Model

The terminal should be treated as a live text source.

### 7.1 TextSource abstraction

```text
TextSource
  Terminal PTY source
  Command output source
  File tail source
  Plugin-provided text source
```

This allows future flexibility without redesign.

A terminal is the first and most important implementation.

### 7.2 Terminal compatibility requirements

Fracterm should aim for strong terminal compatibility.

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

This makes Fracterm more compatible with real-world terminal applications.

---

## 8. More Flexible Subrange Views

The previous subrange view design is good, but it can be generalized.

A subrange view should be a `ProjectionSurface`.

## 8.1 ProjectionSurface

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

This allows a projection to be used as:

- dashboard tile,
- log monitor,
- reading pane,
- search result view,
- accessibility magnifier.

---

## 9. More Ergonomic TypeScript SDK

The previous SDK is good, but it can be made more elegant by using a declarative, typed, schema-driven API.

## 9.1 Direct TypeScript loading

Fracterm should load these directly:

```text
.ts
.mts
.js
.mjs
```

No mandatory build step.

SWC transpiles TypeScript to JavaScript at load time.

Type checking remains a development-time concern.

---

## 9.2 Plugin manifest in TypeScript

Instead of requiring JSON only, allow a TypeScript manifest.

Example file:

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

JSON manifests can still be supported for simple cases, but TypeScript manifests are more ergonomic.

---

## 9.3 Typed commands

Commands should be typed and reusable.

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

Benefits:

- command palette integration,
- keybinding integration,
- plugin integration,
- typed parameters,
- easier testing,
- easier documentation.

---

## 9.4 Typed events

Use a typed event bus.

```ts
ctx.events.on("workspace:objectCreated", event => {
  ctx.log.info("Created object", event.nodeId);
});

ctx.events.on("terminal:output", event => {
  // throttled, permission-checked
});
```

Terminal output events must be throttled and permission-gated.

Plugins should not receive raw high-throughput terminal output unless explicitly permitted and rate-limited.

---

## 9.5 Declarative widget API

The previous widget API uses imperative drawing.

That is fine, but a more ergonomic API is declarative display lists.

Example:

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

This is more ergonomic than raw draw callbacks.

Still allow an advanced immediate-mode escape hatch:

```ts
drawAdvanced(ctx) {
  // custom immediate-mode display list
}
```

---

## 9.6 Settings UI generation

If a plugin defines settings with schemas, Fracterm should automatically generate the options UI.

Example:

```ts
settings: {
  refreshMs: {
    type: "number",
    default: 1000,
    min: 100,
    max: 10000,
  },

  timezone: {
    type: "string",
    default: "local",
  },

  showSeconds: {
    type: "boolean",
    default: true,
  },
}
```

The border options menu can then show:

```text
Refresh: [1000]
Timezone: [local]
Show seconds: [x]
```

This makes plugins feel native.

---

## 10. More Elegant Configuration

Configuration should also be TypeScript-first.

Default path:

```text
~/.config/fracterm/fracterm.config.ts
```

Example:

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

  profiles: {
    "big-text": {
      font: {
        size: 24,
        weight: 500,
      },
      theme: {
        background: "#000000",
        foreground: "#ffffff",
      },
    },
  },
});
```

Terminal profiles are a useful ergonomic addition.

Examples:

```text
default
big-text
ssh
logs
presentation
high-contrast
```

---

## 11. More Flexible Object Arrangement

The workspace should support dashboard ergonomics.

## 11.1 Groups

Allow grouping nodes.

```text
Group
  Terminal
  SubrangeView
  Widget
```

A group can be:

- moved,
- zoomed,
- saved as a layout fragment,
- collapsed,
- aligned,
- distributed.

## 11.2 Snapping

Optional snapping features:

```text
grid snapping
edge snapping
alignment guides
equal spacing
object distribution
```

## 11.3 Arrange commands

Add commands:

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

This makes dashboard creation much easier.

## 11.4 Camera bookmarks

Users should be able to save camera positions.

Examples:

```text
Overview
Logs
Build
Monitoring
Reading
```

A camera bookmark is just another lens target.

---

## 12. More Elegant Border and HUD Design

The border menu should be minimal but powerful.

## 12.1 Border controls

Visible on hover/select:

```text
[x] close
[⋯] options
resize handles
drag handle/title bar
```

The options popover should include:

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

## 12.2 HUD

HUD should remain auto-hiding.

HUD actions:

```text
New Terminal
Command Palette
Save Layout
Restore Layout
Toggle Effects
Plugin Console
Help
```

The command palette should be the universal entry point.

Every action should be available from the palette.

This is more elegant than burying features in menus.

---

## 13. More Compatible JavaScript Runtime

The previous spec chooses V8. That remains the best primary engine.

But for maximum flexibility, define a small Rust-side abstraction:

```rust
trait ScriptHost {
    fn load_plugin(&mut self, manifest: PluginManifest) -> Result<PluginId>;
    fn unload_plugin(&mut self, id: PluginId) -> Result<()>;
    fn call(&mut self, id: PluginId, method: &str, args: Value) -> Result<Value>;
    fn dispatch_event(&mut self, event: Event) -> Result<()>;
}
```

Primary implementation:

```text
V8ScriptHost
```

Optional future implementation:

```text
QuickJsScriptHost
```

The public plugin API remains JavaScript/TypeScript.

This improves long-term compatibility without burdening plugin authors.

---

## 14. More Web-Compatible Plugin Environment

Plugins should feel familiar to JavaScript developers.

Provide standard Web-like globals where safe:

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

Do not provide:

```text
DOM
Node.js built-ins
raw process APIs
raw filesystem APIs
raw OpenGL APIs
```

Optional host-mediated APIs can be permission-gated:

```text
fetch
storage
clipboard
terminal read/write
filesystem sandbox
process spawning
```

This gives good ergonomics without giving up security.

---

## 15. More Flexible Permissions

Permissions should be scoped, not just boolean.

Example manifest:

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

This is more secure and more flexible.

---

## 16. More Elegant Plugin Lifecycle

Plugins should have a clear lifecycle.

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

All registrations return disposables.

```ts
const disposable = ctx.commands.register(...);

ctx.onDeactivate(() => {
  disposable.dispose();
});
```

The host should also automatically dispose plugin resources on unload.

---

## 17. More Elegant Command System

Every action should be a command.

Examples:

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

- keybindings can bind to commands,
- plugins can invoke commands,
- HUD uses commands,
- command palette uses commands,
- tests can invoke commands,
- future CLI can invoke commands.

This makes the system much more elegant.

---

## 18. More Ergonomic Accessibility Features

The near-sightedness zoom should become a first-class accessibility feature.

## 18.1 Reading lens

A reading lens can reflow text instead of merely magnifying terminal cells.

Features:

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

## 18.2 Quick reading action

Context menu:

```text
Read selection
Read current line
Read last 50 lines
Read filtered selection
```

## 18.3 Accessibility options

Configuration should include:

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

This makes Fracterm more useful and more humane.

---

## 19. More Compatible Linux Behavior

Fracterm should feel native on Linux.

## 19.1 XDG directories

Use:

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

## 19.2 Clipboard

Support:

```text
clipboard
primary selection
middle-click paste where appropriate
OSC 52 with permission
```

## 19.3 HiDPI

Support:

```text
integer scaling
fractional scaling where available
per-monitor DPI changes
crisp text at high DPI
```

## 19.4 Desktop integration

Provide:

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

## 20. More Elegant Persistence

Layouts should store the scene graph, not just terminal positions.

A layout should include:

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

This makes restore behavior more flexible.

---

## 21. More Flexible Plugin Widget System

Widgets should be able to participate in workspace life without being terminals.

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

Widgets should not directly access OpenGL.

They produce display lists.

This keeps plugins safe and the renderer fast.

---

## 22. More Elegant Search

Search should be built into terminals and projections.

Features:

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

## 23. More Elegant Context Menu

Instead of overloading right-click, use:

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

## 24. More Elegant Developer Tooling

Fracterm should be pleasant for plugin developers.

## 24.1 Generated types

Generate TypeScript definitions from the Rust API.

```text
fracterm.d.ts
fracterm/config.d.ts
fracterm/plugin.d.ts
fracterm/widget.d.ts
```

This prevents documentation drift.

## 24.2 In-app plugin console

The plugin console should show:

```text
console.log
console.warn
console.error
plugin load state
permission grants
runtime exceptions
source-mapped stack traces
```

## 24.3 Hot reload

Development mode should watch plugin files:

```text
edit main.ts
  -> SWC transpile
  -> deactivate plugin
  -> reload plugin
  -> restore workspace state where possible
```

## 24.4 Plugin doctor

Provide a diagnostic command:

```text
fracterm doctor
```

It should check:

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

## 25. More Compatible Testing Strategy

The improved spec should include stronger compatibility testing.

## 25.1 Terminal tests

Use:

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

## 25.2 Rendering tests

Use golden-image tests for:

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

## 25.3 Plugin API tests

Run TypeScript plugin contract tests:

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

## 26. Improved Milestones

The implementation order can also be made more elegant.

### Milestone 1: Core canvas primitives

Build:

```text
workspace
camera
node
transform
style
OpenGL renderer
```

Exit:

```text
rectangles and text can be zoomed and panned smoothly
```

### Milestone 2: Text surface rendering

Build:

```text
glyph atlas
text batching
sharp zoom strategy
```

Exit:

```text
text remains crisp during deep zoom
```

### Milestone 3: Terminal surface

Build:

```text
PTY
terminal parser
grid
damage tracking
keyboard input
```

Exit:

```text
interactive bash works inside a canvas node
```

### Milestone 4: Projection system

Build:

```text
surface projections
row/column selectors
snapshot/live modes
filters
```

Exit:

```text
terminal subrange views can be pinned and arranged
```

### Milestone 5: Lens zooming

Build:

```text
zoom to object
zoom to rectangle
zoom to terminal range
reading lens
camera bookmarks
```

Exit:

```text
any zoom can become a pinned view
```

### Milestone 6: JavaScript host

Build:

```text
V8 isolate
module loader
SWC TypeScript transpile
plugin lifecycle
permissions
console
```

Exit:

```text
TypeScript plugin can register commands and widgets
```

### Milestone 7: Typed SDK

Build:

```text
defineConfig
definePlugin
defineWidget
typed commands
typed events
generated d.ts
```

Exit:

```text
pleasant TypeScript development experience
```

### Milestone 8: Dashboard ergonomics

Build:

```text
groups
snapping
alignment
arrange commands
layout save/restore
```

Exit:

```text
users can build dashboards quickly
```

### Milestone 9: Accessibility and polish

Build:

```text
reading mode
high contrast themes
reduced motion
large cursor
font profiles
```

Exit:

```text
near-sighted reading workflow is excellent
```

---

## 27. What Specifically Becomes Better?

### More ergonomic

Because of:

- TypeScript-first SDK,
- declarative widgets,
- typed commands,
- generated types,
- hot reload,
- command palette,
- settings schema UI,
- reading mode,
- context-aware menus.

### More compatible

Because of:

- stronger terminal compatibility,
- Unicode-aware rendering,
- fontconfig/HarfBuzz integration,
- X11/Wayland support,
- HiDPI support,
- OpenGL capability detection,
- stable plugin API versioning,
- optional engine abstraction.

### More flexible

Because of:

- surfaces and projections,
- any text source can become a view,
- projections can be live or snapshot,
- projections can be filtered,
- any zoom can be pinned,
- groups and layouts,
- camera bookmarks,
- terminal profiles,
- plugin widgets with settings.

### More elegant

Because of:

- fewer special cases,
- unified lens model,
- command-driven architecture,
- capability-based permissions,
- display-list plugin rendering,
- scene graph instead of ad-hoc object types,
- consistent lifecycle and disposal.

---

## 28. Final Recommended Improved Stack

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

## 29. Short Answer

Yes.

The best version of Fracterm is not merely:

```text
a terminal emulator with zoom
```

It is:

```text
a spatial lens system for live text surfaces
```

where terminals are the primary live text source, subrange views are projections, zooming is lens navigation, plugins are typed TypeScript extensions, and every action is a command.


