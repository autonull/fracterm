//! Fracterm binary entry point.

use fracterm::app::{default_config, App, CliArgs};
use fracterm::config::Config;
use fracterm::window::run_windowed;

/// Load `~/.config/fracterm/fracterm.config.ts` (XDG-aware) when present;
/// fall back to built-in defaults on any error so a bad config never
/// bricks startup.
fn load_config() -> Config {
    let path = Config::config_path();
    if path.exists() {
        match Config::load_from_ts(&path) {
            Ok(config) => {
                eprintln!("config loaded from {}", path.display());
                return config;
            }
            Err(e) => eprintln!("config {} failed ({e}); using defaults", path.display()),
        }
    }
    default_config()
}

fn main() {
    let args = CliArgs::parse();
    
    if args.show_help {
        CliArgs::print_help();
        return;
    }
    
    if args.show_version {
        CliArgs::print_version();
        return;
    }
    
    if args.print_config {
        args.print_effective_config();
        return;
    }
    
    if args.doctor {
        args.run_doctor();
        return;
    }

    let config = load_config();
    let app = App::new(config);

    // Try the OpenGL canvas; degrade to the headless logical loop when no
    // 3.3 core context is available (capability-based graceful fallback).
    if run_windowed(app).is_err() {
        let mut app = App::new(load_config());
        app.run();
    }
}
