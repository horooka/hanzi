mod audio;
mod db;
mod pinyin;
mod strokes;
mod ui;
mod writing;

use gtk::prelude::*;

const APP_ID: &str = "io.github.horooka.hanzi";
const RESOURCE_PREFIX: &str = "/io/github/horooka/hanzi";

fn main() -> glib::ExitCode {
    gio::resources_register_include!("hanzi.gresource")
        .expect("failed to register GResource bundle");

    let app = adw::Application::builder().application_id(APP_ID).build();
    app.connect_startup(|_| load_css());
    app.connect_activate(ui::build);
    app.run()
}

fn load_css() {
    let provider = gtk::CssProvider::new();
    provider.load_from_resource(&format!("{RESOURCE_PREFIX}/style.css"));

    if let Some(display) = gtk::gdk::Display::default() {
        gtk::IconTheme::for_display(&display)
            .add_resource_path(&format!("{RESOURCE_PREFIX}/icons/hicolor"));

        gtk::style_context_add_provider_for_display(
            &display,
            &provider,
            gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
        );
    }
}
