//! Fracterm binary entry point.

use fracterm::app::{default_config, App, CliArgs};
use fracterm::window::run_windowed;

fn main() {
    let args = CliArgs::default();
    let config = default_config();
    let app = App::new(config);

    // Try the OpenGL canvas; degrade to the headless logical loop when no
    // 3.3 core context is available (capability-based graceful fallback).
    if run_windowed(app).is_err() {
        let mut app = App::new(default_config());
        app.run();
    }
    let _ = args;
}
