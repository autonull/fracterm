//! Fracterm binary entry point.

use fracterm::app::{App, CliArgs, default_config};

fn main() {
    let args = CliArgs::default();
    let config = default_config();
    let mut app = App::new(config);
    app.run();
    let _ = args;
}