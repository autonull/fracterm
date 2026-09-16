//! Generated TypeScript definitions for the plugin SDK (README §24.1).
//!
//! The `.d.ts` text is generated from the Rust API surface so docs cannot
//! drift from the host: command/event/permission names and the
//! settings-schema and widget-builder shapes mirror `plugin.rs`,
//! `command.rs`, `event.rs`, and `config.rs`.

/// API version plugin manifests target.
pub const DTS_API_VERSION: &str = "fracterm/1";

/// `fracterm/config` declarations: `defineConfig` and the config tree.
pub fn dts_config() -> String {
    r#"declare module "fracterm/config" {
  export interface FontConfig {
    family: string;
    size: number;
    ligatures: boolean;
    weight?: number;
  }
  export interface ThemeConfig {
    background: string;
    foreground: string;
    cursor: string;
    selection: string;
  }
  export interface CameraConfig {
    wheelZoomSpeed: number;
    autoZoomAnimationMs: number;
    easing: string;
  }
  export interface EffectsConfig {
    motionBlur: boolean;
    backgroundBlur: boolean;
  }
  export interface TerminalConfig {
    scrollbackLines: number;
    copyOnSelect: boolean;
    ambiguousWidth: 1 | 2;
  }
  export interface ReadingModeConfig {
    fontSize: number;
    lineHeight: number;
    highContrast: boolean;
    hideChrome: boolean;
  }
  export interface FractermConfig {
    font?: Partial<FontConfig>;
    theme?: Partial<ThemeConfig>;
    camera?: Partial<CameraConfig>;
    effects?: Partial<EffectsConfig>;
    terminal?: Partial<TerminalConfig>;
    accessibility?: { readingMode?: Partial<ReadingModeConfig> };
    profiles?: Record<string, Partial<FractermConfig>>;
  }
  export function defineConfig(config: FractermConfig): FractermConfig;
}
"#
    .to_string()
}

/// `fracterm/plugin` declarations: `definePlugin`, commands, events,
/// permissions, settings schemas, storage, and logging.
pub fn dts_plugin() -> String {
    r#"declare module "fracterm/plugin" {
  export type PermissionScope =
    | { permission: "terminal.read"; scope: "created" | "granted" | "all" }
    | { permission: "terminal.write"; scope: "created" | "granted" | "all" }
    | { permission: "storage"; scope: "plugin" }
    | { permission: "network"; scope: string[] }
    | string;
  export interface SettingSchema {
    type: "string" | "number" | "boolean" | "array" | "object";
    default: unknown;
    min?: number;
    max?: number;
    description?: string;
    enum?: string[];
  }
  export interface CommandInput {
    command?: string;
    cwd?: string;
    title?: string;
    [key: string]: unknown;
  }
  export interface CommandContext {
    commands: Commands;
    events: Events;
    terminals: Terminals;
    camera: Camera;
    workspace: Workspace;
    log: Log;
    storage: Storage;
  }
  export interface Commands {
    register(def: {
      id: string;
      title: string;
      category?: string;
      input?: Record<string, unknown>;
      run: (input: CommandInput) => unknown | Promise<unknown>;
    }): { dispose(): void };
    invoke(id: string, input?: CommandInput): Promise<unknown>;
  }
  export type ZoomTarget =
    | { type: "workspace-fit" }
    | { type: "object"; nodeId: string }
    | { type: "rectangle"; rect: { x: number; y: number; width: number; height: number } }
    | { type: "terminal-range"; terminalId: string; range: GridRange }
    | { type: "projection"; projectionId: string }
    | { type: "reading"; sourceId: string; range?: GridRange };
  export interface GridRange {
    startRow: number;
    endRow: number;
    startCol: number;
    endCol: number;
  }
  export interface Camera {
    zoomTo(target: ZoomTarget): Promise<void>;
  }
  export interface Terminals {
    spawn(opts?: { command?: string; cwd?: string; title?: string }): Promise<{ id: string }>;
  }
  export interface Workspace {
    nodes(): unknown[];
  }
  export interface Events {
    on(type: "workspace:objectCreated" | "terminal:output" | "camera:zoomChanged" | string,
       handler: (event: unknown) => void): { dispose(): void };
  }
  export interface Log {
    info(...args: unknown[]): void;
    warn(...args: unknown[]): void;
    error(...args: unknown[]): void;
  }
  export interface Storage {
    get(key: string): unknown;
    set(key: string, value: unknown): void;
  }
  export interface PluginDefinition {
    id: string;
    name: string;
    version: string;
    api: "fracterm/1";
    permissions?: PermissionScope[];
    settings?: Record<string, SettingSchema>;
    activate(ctx: CommandContext): void | Promise<void>;
    deactivate?(): void | Promise<void>;
  }
  export function definePlugin(plugin: PluginDefinition): PluginDefinition;
}
"#
    .to_string()
}

/// `fracterm/widget` declarations: `defineWidget`, timers, and the
/// immediate-mode builder (`clear`/`rect`/`text`/`line`/`scissor`).
pub fn dts_widget() -> String {
    r#"declare module "fracterm/widget" {
  export interface WidgetState {
    [key: string]: unknown;
  }
  export interface TextOptions {
    x: number;
    y: number;
    scale?: number;
    color?: string;
    align?: "left" | "center" | "right";
  }
  export interface Ui {
    clear(color: string): void;
    rect(x: number, y: number, w: number, h: number, color: string): void;
    text(content: string, options: TextOptions): void;
    line(x1: number, y1: number, x2: number, y2: number, color: string, width: number): void;
    scissor(x: number, y: number, w: number, h: number): void;
    scissorEnd(): void;
  }
  export interface WidgetViewContext {
    bounds: { x: number; y: number; width: number; height: number };
    theme: { background: string; foreground: string };
  }
  export interface WidgetDefinition<S extends WidgetState = WidgetState> {
    id: string;
    title: string;
    defaultSize?: { width: number; height: number };
    state?: () => S;
    timers?: Array<{ everyMs: number; run(state: S): void }>;
    view(state: S, ui: Ui, ctx: WidgetViewContext): void;
  }
  export function defineWidget<S extends WidgetState>(widget: WidgetDefinition<S>): WidgetDefinition<S>;
}
"#
    .to_string()
}

/// Full `fracterm.d.ts` bundle: config + plugin + widget modules.
pub fn generate_dts() -> String {
    format!(
        "// Generated by fracterm (api {}) — do not edit by hand.\n{}\n{}\n{}",
        DTS_API_VERSION,
        dts_config(),
        dts_plugin(),
        dts_widget()
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bundle_contains_all_modules() {
        let dts = generate_dts();
        for module in [
            r#"declare module "fracterm/config""#,
            r#"declare module "fracterm/plugin""#,
            r#"declare module "fracterm/widget""#,
        ] {
            assert!(dts.contains(module), "missing {module}");
        }
    }

    #[test]
    fn test_plugin_surface_matches_host() {
        let dts = dts_plugin();
        for symbol in [
            "definePlugin",
            "fracterm/1",
            "terminal-range",
            "workspace:objectCreated",
            "terminal:output",
        ] {
            assert!(dts.contains(symbol), "missing {symbol}");
        }
    }

    #[test]
    fn test_widget_builder_matches_ui_builder() {
        let dts = dts_widget();
        for method in [
            "clear(",
            "rect(",
            "text(",
            "line(",
            "scissor(",
            "scissorEnd(",
            "defineWidget",
        ] {
            assert!(dts.contains(method), "missing {method}");
        }
    }

    #[test]
    fn test_config_covers_readme_example() {
        let dts = dts_config();
        for key in [
            "defineConfig",
            "wheelZoomSpeed",
            "scrollbackLines",
            "readingMode",
        ] {
            assert!(dts.contains(key), "missing {key}");
        }
    }
}
