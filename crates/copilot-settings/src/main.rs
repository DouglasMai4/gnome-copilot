mod daemon;
mod pages;

use adw::prelude::*;
use copilot_core::SETTINGS_APP_ID;

fn main() -> gtk::glib::ExitCode {
    let args: Vec<String> = std::env::args().collect();
    let page = args
        .iter()
        .position(|a| a == "--page")
        .and_then(|i| args.get(i + 1))
        .cloned()
        .unwrap_or_else(|| "general".into());

    let app = adw::Application::builder().application_id(SETTINGS_APP_ID).build();
    app.connect_activate(move |app| {
        if let Some(window) = app.active_window() {
            window.present();
            return;
        }
        pages::build_window(app, &page).present();
    });

    app.run_with_args(&args[..1])
}
