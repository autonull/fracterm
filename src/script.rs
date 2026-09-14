//! Script host abstraction (README M6): a swappable engine boundary for
//! plugin execution. Primary implementation: QuickJS (`rquickjs`).
//! V8/deno_core can be added behind the same `ScriptHost` trait.

use std::collections::HashMap;

use rquickjs::{Context, Runtime};

use crate::event::Event;

/// Opaque plugin identifier.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct PluginId(pub u64);

/// Scoped permission request (README §15).
#[derive(Debug, Clone, PartialEq)]
pub enum PermissionRequest {
    Unscoped(String),
    Scoped { permission: String, scope: String },
}

/// Plugin manifest (TS/JSON side serialized to this struct).
#[derive(Debug, Clone, Default)]
pub struct PluginManifest {
    pub id: String,
    pub name: String,
    pub version: String,
    /// Target API version, e.g. "fracterm/1"
    pub api: String,
    pub permissions: Vec<PermissionRequest>,
    /// Plugin source code. TypeScript is transpiled to JS by SWC at load
    /// time (README §9.1); JavaScript is evaluated as-is.
    pub source: String,
    /// Source file name, used to detect TypeScript (`.ts`, `.mts`, `.tsx`).
    pub source_path: Option<String>,
}

/// Decide whether source needs SWC TypeScript stripping based on its path.
fn is_typescript(path: Option<&str>) -> bool {
    path.map(|p| {
        let lower = p.to_ascii_lowercase();
        lower.ends_with(".ts") || lower.ends_with(".mts") || lower.ends_with(".tsx")
    })
    .unwrap_or(false)
}

/// Engine-agnostic plugin host boundary (README §13).
pub trait ScriptHost {
    fn load_plugin(&mut self, manifest: PluginManifest) -> Result<PluginId, String>;
    fn unload_plugin(&mut self, id: &PluginId) -> Result<(), String>;
    fn call(
        &mut self,
        id: &PluginId,
        method: &str,
        args: serde_json::Value,
    ) -> Result<serde_json::Value, String>;
    fn dispatch_event(&mut self, event: &Event) -> Result<(), String>;
    fn loaded_plugins(&self) -> Vec<PluginId>;
}

/// Permission check over a manifest's requests.
fn manifest_permissions_allows(
    manifest: &PluginManifest,
    permission: &str,
    scope: Option<&str>,
) -> bool {
    manifest.permissions.iter().any(|req| match req {
        PermissionRequest::Unscoped(p) => p == permission,
        PermissionRequest::Scoped {
            permission: p,
            scope: s,
        } => p == permission && scope.is_some_and(|want| want == s),
    })
}

/// Per-plugin runtime state.
struct PluginInstance {
    /// Kept alive by the context; named explicitly for clarity.
    _runtime: Runtime,
    context: Context,
    manifest: PluginManifest,
}

/// QuickJS-backed script host. Each plugin gets an isolated runtime.
pub struct QuickJsScriptHost {
    plugins: HashMap<PluginId, PluginInstance>,
    next_id: u64,
    /// Command ids registered by plugins (for the palette/keybindings later).
    pub registered_commands: Vec<String>,
}

impl QuickJsScriptHost {
    pub fn new() -> Self {
        Self {
            plugins: HashMap::new(),
            next_id: 1,
            registered_commands: Vec::new(),
        }
    }

    /// Install web-like globals + the `fracterm` host API into a context.
    fn install_globals<'js>(
        &self,
        ctx: &rquickjs::Ctx<'js>,
        plugin_id: &PluginId,
        manifest: &PluginManifest,
    ) -> Result<(), String> {
        let tag = format!("{plugin_id:?}");
        // Permission gate: command registration requires commands.register.
        // (Checked against the manifest directly — the plugin isn't in the
        // registry yet at install time.)
        let can_register_commands =
            manifest_permissions_allows(manifest, "commands.register", None);
        let tag_err = tag.clone();
        use rquickjs::function::Func;
        let console = rquickjs::Object::new(ctx.clone()).map_err(|e| e.to_string())?;
        console
            .set(
                "log",
                Func::from(move |msg: String| eprintln!("[plugin {tag}] {msg}")),
            )
            .map_err(|e| e.to_string())?;
        console
            .set(
                "error",
                Func::from(move |msg: String| eprintln!("[plugin {tag_err}] ERROR: {msg}")),
            )
            .map_err(|e| e.to_string())?;

        // fracterm host API surface.
        let host = rquickjs::Object::new(ctx.clone()).map_err(|e| e.to_string())?;
        let commands = rquickjs::Object::new(ctx.clone()).map_err(|e| e.to_string())?;
        commands
            .set(
                "register",
                Func::from(move |spec: rquickjs::Object<'js>| -> rquickjs::Result<()> {
                    let id: String = spec.get("id").unwrap_or_default();
                    if !can_register_commands {
                        return Err(rquickjs::Error::FromJs {
                            from: "plugin",
                            to: "host",
                            message: Some(format!(
                                "permission denied: commands.register (requested for '{id}')"
                            )),
                        });
                    }
                    eprintln!("[host] command registered: {id}");
                    Ok(())
                }),
            )
            .map_err(|e| e.to_string())?;
        host.set("commands", commands).map_err(|e| e.to_string())?;
        ctx.globals()
            .set("console", console)
            .map_err(|e| e.to_string())?;
        ctx.globals()
            .set("fracterm", host)
            .map_err(|e| e.to_string())?;
        Ok(())
    }
}

impl QuickJsScriptHost {
    /// Check whether a plugin holds a permission (optionally scoped).
    /// Unscoped grants satisfy any scope of that permission; scoped grants
    /// satisfy only the exact scope.
    pub fn has_permission(&self, id: &PluginId, permission: &str, scope: Option<&str>) -> bool {
        let Some(inst) = self.plugins.get(id) else {
            return false;
        };
        inst.manifest.permissions.iter().any(|req| match req {
            PermissionRequest::Unscoped(p) => p == permission,
            PermissionRequest::Scoped {
                permission: p,
                scope: s,
            } => p == permission && scope.is_some_and(|want| want == s),
        })
    }

    /// Get a loaded plugin's manifest.
    pub fn manifest(&self, id: &PluginId) -> Option<&PluginManifest> {
        self.plugins.get(id).map(|p| &p.manifest)
    }
}

impl Default for QuickJsScriptHost {
    fn default() -> Self {
        Self::new()
    }
}

impl ScriptHost for QuickJsScriptHost {
    fn load_plugin(&mut self, manifest: PluginManifest) -> Result<PluginId, String> {
        let runtime = Runtime::new().map_err(|e| e.to_string())?;
        let context = Context::full(&runtime).map_err(|e| e.to_string())?;
        let id = PluginId(self.next_id);
        self.next_id += 1;

        // TypeScript transpilation (SWC) at load time, no build step.
        let source = if is_typescript(manifest.source_path.as_deref()) {
            transpile_ts(
                &manifest.source,
                manifest.source_path.as_deref().unwrap_or("plugin.ts"),
            )?
        } else {
            manifest.source.clone()
        };

        context.with(|ctx| -> Result<(), String> {
            self.install_globals(&ctx, &id, &manifest)?;
            // Evaluate plugin source; catch and report syntax errors.
            let _: () = ctx.eval(source.as_bytes()).map_err(|e| e.to_string())?;
            // Invoke activate(ctx) if the plugin defines it.
            let has_activate: bool = ctx
                .globals()
                .contains_key("activate")
                .map_err(|e| e.to_string())?;
            if has_activate {
                eprintln!("ACTIVATE-PRE");
                let activate: rquickjs::Function =
                    ctx.globals().get("activate").map_err(|e| e.to_string())?;
                let fracterm: rquickjs::Value =
                    ctx.globals().get("fracterm").map_err(|e| e.to_string())?;
                let _: () = activate.call((fracterm,)).map_err(|e| {
                    let caught = ctx.catch();
                    let detail = caught
                        .as_object()
                        .and_then(|o| o.get::<&str, String>("message").ok())
                        .unwrap_or_else(|| format!("{e}"));
                    detail
                })?;
            }
            Ok(())
        })?;

        self.plugins.insert(
            id.clone(),
            PluginInstance {
                _runtime: runtime,
                context,
                manifest,
            },
        );
        Ok(id)
    }

    fn unload_plugin(&mut self, id: &PluginId) -> Result<(), String> {
        let instance = self
            .plugins
            .remove(id)
            .ok_or_else(|| format!("plugin {id:?} not loaded"))?;
        // Graceful deactivation hook, best-effort.
        instance.context.with(|ctx| -> Result<(), String> {
            let has_deactivate: bool = ctx
                .globals()
                .contains_key("deactivate")
                .map_err(|e| e.to_string())?;
            if has_deactivate {
                let deactivate: rquickjs::Function =
                    ctx.globals().get("deactivate").map_err(|e| e.to_string())?;
                let _: () = deactivate.call(()).map_err(|e| e.to_string())?;
            }
            Ok(())
        })?;
        // Dropping the instance tears down the runtime (disposables cleanup).
        Ok(())
    }

    fn call(
        &mut self,
        id: &PluginId,
        method: &str,
        args: serde_json::Value,
    ) -> Result<serde_json::Value, String> {
        let instance = self
            .plugins
            .get_mut(id)
            .ok_or_else(|| format!("plugin {id:?} not loaded"))?;
        let json = serde_json::to_string(&args).map_err(|e| e.to_string())?;
        instance.context.with(|ctx| -> Result<serde_json::Value, String> {
            let src = r#"
                (function(method, json) {
                    return JSON.stringify(globalThis[method] ? globalThis[method](JSON.parse(json)) : null);
                })
            "#;
            let f: rquickjs::Function = ctx.eval(src.as_bytes()).map_err(|e| e.to_string())?;
            let result: String = f.call((method, json)).map_err(|e| e.to_string())?;
            serde_json::from_str(&result).map_err(|e| e.to_string())
        })
    }

    fn dispatch_event(&mut self, event: &Event) -> Result<(), String> {
        let payload = serde_json::to_string(event).unwrap_or_else(|_| "{}".into());
        let ids: Vec<PluginId> = self.plugins.keys().cloned().collect();
        for id in ids {
            let instance = self.plugins.get_mut(&id).expect("id from keys");
            instance.context.with(|ctx| -> Result<(), String> {
                let has_handler: bool = ctx
                    .globals()
                    .contains_key("onEvent")
                    .map_err(|e| e.to_string())?;
                if has_handler {
                    let handler: rquickjs::Function =
                        ctx.globals().get("onEvent").map_err(|e| e.to_string())?;
                    let _: () = handler
                        .call((payload.as_str(),))
                        .map_err(|e| e.to_string())?;
                }
                Ok(())
            })?;
        }
        Ok(())
    }

    fn loaded_plugins(&self) -> Vec<PluginId> {
        self.plugins.keys().cloned().collect()
    }
}

/// Transpile TypeScript to JavaScript at load time using SWC (README §9.1):
/// strip type annotations, enums, and namespaces; no build step required.
pub fn transpile_ts(source: &str, filename: &str) -> Result<String, String> {
    use swc_common::{sync::Lrc, FileName, Globals, Mark, SourceMap, GLOBALS};
    use swc_ecma_codegen::{text_writer::JsWriter, Emitter};
    use swc_ecma_parser::{lexer::Lexer, Parser, StringInput, Syntax, TsSyntax};

    let cm: Lrc<SourceMap> = Lrc::new(SourceMap::new(Default::default()));
    let globals = Globals::new();
    GLOBALS.set(&globals, || {
        let fm = cm.new_source_file(
            FileName::Custom(filename.to_string()).into(),
            source.to_string(),
        );

        let lexer = Lexer::new(
            Syntax::Typescript(TsSyntax {
                decorators: true,
                ..Default::default()
            }),
            swc_ecma_ast::EsVersion::Es2022,
            StringInput::from(&*fm),
            None,
        );
        let mut parser = Parser::new_from(lexer);
        let module = parser
            .parse_module()
            .map_err(|e| format!("parse error in {filename}: {e:?}"))?;

        let unresolved_mark = Mark::new();
        let top_level_mark = Mark::new();
        let mut ts = strip(unresolved_mark, top_level_mark);
        let mut program = swc_ecma_ast::Program::Module(module);
        swc_ecma_ast::Pass::process(&mut ts, &mut program);
        let swc_ecma_ast::Program::Module(module) = program else {
            return Err("unexpected program shape after TS strip".into());
        };

        let mut buf = Vec::new();
        {
            let writer = JsWriter::new(cm.clone(), "\n", &mut buf, None);
            let mut emitter = Emitter {
                cfg: Default::default(),
                cm: cm.clone(),
                comments: None,
                wr: Box::new(writer),
            };
            emitter.emit_module(&module).map_err(|e| e.to_string())?;
        }
        String::from_utf8(buf).map_err(|e| e.to_string())
    })
}

use swc_ecma_transforms_typescript::strip;

#[cfg(test)]
mod tests {
    use super::*;

    fn host() -> QuickJsScriptHost {
        QuickJsScriptHost::new()
    }

    #[test]
    fn test_load_and_call() {
        let mut h = host();
        let id = h
            .load_plugin(PluginManifest {
                id: "test.adder".into(),
                name: "Adder".into(),
                version: "0.1.0".into(),
                api: "fracterm/1".into(),
                permissions: vec![],
                source: "function double(n) { return n * 2; }".into(),
                source_path: Some("double.js".into()),
            })
            .unwrap();
        let result = h.call(&id, "double", serde_json::json!(21)).unwrap();
        assert_eq!(result, serde_json::json!(42));
    }

    #[test]
    fn test_activate_hook_and_command_registration() {
        let mut h = host();
        let id = h
            .load_plugin(PluginManifest {
                id: "test.hello".into(),
                name: "Hello".into(),
                version: "0.1.0".into(),
                api: "fracterm/1".into(),
                permissions: vec![PermissionRequest::Unscoped("commands.register".into())],
                source_path: Some("hello.js".into()),
                source: r#"
                    var activated = false;
                    function activate(ctx) { activated = true; ctx.commands.register({ id: "test.ping" }); }
                    function ping() { return "pong"; }
                "#
                .into(),
            })
            .unwrap();
        let result = h.call(&id, "ping", serde_json::json!(null)).unwrap();
        assert_eq!(result, serde_json::json!("pong"));
    }

    #[test]
    fn test_unload_calls_deactivate() {
        let mut h = host();
        let id = h
            .load_plugin(PluginManifest {
                source: "function deactivate() { globalThis.done = true; }".into(),
                ..Default::default()
            })
            .unwrap();
        assert!(h.unload_plugin(&id).is_ok());
        assert!(h.loaded_plugins().is_empty());
    }

    #[test]
    fn test_unload_unknown_is_error() {
        let mut h = host();
        assert!(h.unload_plugin(&PluginId(999)).is_err());
    }

    #[test]
    fn test_event_dispatch() {
        let mut h = host();
        let id = h
            .load_plugin(PluginManifest {
                source: r#"
                    var last = null;
                    function onEvent(json) { last = JSON.parse(json).event_type; }
                    function lastEvent() { return last; }
                "#
                .into(),
                ..Default::default()
            })
            .unwrap();
        let event = Event::new(crate::event::EventType::CommandExecuted);
        h.dispatch_event(&event).unwrap();
        let result = h.call(&id, "lastEvent", serde_json::json!(null)).unwrap();
        // Event serializes with its kind/type field.
        assert_eq!(result, serde_json::json!("CommandExecuted"));
    }

    #[test]
    fn test_scoped_permission_matching() {
        let mut h = host();
        let id = h
            .load_plugin(PluginManifest {
                permissions: vec![
                    PermissionRequest::Unscoped("workspace.read".into()),
                    PermissionRequest::Scoped {
                        permission: "terminal.read".into(),
                        scope: "created".into(),
                    },
                ],
                ..Default::default()
            })
            .unwrap();
        assert!(h.has_permission(&id, "workspace.read", None));
        assert!(h.has_permission(&id, "workspace.read", Some("anything")));
        assert!(h.has_permission(&id, "terminal.read", Some("created")));
        assert!(!h.has_permission(&id, "terminal.read", Some("all")));
        assert!(!h.has_permission(&id, "fs.read", None));
        assert!(!h.has_permission(&PluginId(999), "workspace.read", None));
    }

    #[test]
    fn test_command_registration_permission_gate() {
        let mut h = host();
        // Without the permission, activate() throws on commands.register,
        // which aborts the plugin load.
        assert!(h
            .load_plugin(PluginManifest {
                source: "function activate(ctx) { ctx.commands.register({ id: 'x' }); }".into(),
                ..Default::default()
            })
            .is_err());

        // With the permission, registration succeeds.
        let id = h
            .load_plugin(PluginManifest {
                permissions: vec![PermissionRequest::Unscoped("commands.register".into())],
                source: "function activate(ctx) { ctx.commands.register({ id: 'x' }); }".into(),
                ..Default::default()
            })
            .unwrap();
        assert!(h.manifest(&id).is_some());
    }

    #[test]
    fn test_typescript_transpile_and_load() {
        let mut h = host();
        let ts = r#"
            interface Cfg { interval: number; label: string }
            type Pair = [number, number];

            class Clock {
                private interval: number;
                constructor(cfg: Cfg) { this.interval = cfg.interval; }
                tick(n: number): number { return n + this.interval; }
            }

            function makeLabel(cfg: Cfg): string {
                return `${cfg.label}:${cfg.interval}`;
            }

            function evalPair(p: Pair): number {
                return p[0] + p[1];
            }
        "#;
        let id = h
            .load_plugin(PluginManifest {
                id: "ts.clock".into(),
                name: "Clock".into(),
                version: "0.1.0".into(),
                api: "fracterm/1".into(),
                permissions: vec![],
                source: ts.to_string(),
                source_path: Some("fracterm.plugin.ts".into()),
            })
            .expect("TS plugin should load");
        let sum = h.call(&id, "evalPair", serde_json::json!([3, 4])).unwrap();
        assert_eq!(sum, serde_json::json!(7));
        let label = h
            .call(
                &id,
                "makeLabel",
                serde_json::json!({"interval": 5, "label": "t"}),
            )
            .unwrap();
        assert_eq!(label, serde_json::json!("t:5"));
    }

    #[test]
    fn test_ts_enums_and_generics() {
        let mut h = host();
        let ts = r#"
            enum Level { Info = 1, Warn = 2 }
            function levelName(l: Level): string {
                return l === Level.Warn ? "warn" : "info";
            }
            function first<T>(items: T[]): T { return items[0]; }
        "#;
        let id = h
            .load_plugin(PluginManifest {
                source: ts.to_string(),
                source_path: Some("p.ts".into()),
                ..Default::default()
            })
            .expect("enum/generic TS loads");
        let name = h.call(&id, "levelName", serde_json::json!(2)).unwrap();
        assert_eq!(name, serde_json::json!("warn"));
        let first = h.call(&id, "first", serde_json::json!([9, 8])).unwrap();
        assert_eq!(first, serde_json::json!(9));
    }

    #[test]
    fn test_is_typescript_detection() {
        assert!(is_typescript(Some("plugin.ts")));
        assert!(is_typescript(Some("mod.mts")));
        assert!(!is_typescript(Some("plugin.js")));
        assert!(!is_typescript(Some("data.json")));
        assert!(!is_typescript(None));
    }

    #[test]
    fn test_syntax_error_is_reported() {
        let mut h = host();
        let err = h
            .load_plugin(PluginManifest {
                source: "function broken( {".into(),
                ..Default::default()
            })
            .unwrap_err();
        assert!(!err.is_empty());
    }
}
