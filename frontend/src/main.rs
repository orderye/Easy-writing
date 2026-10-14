mod app;
mod bridge;
mod components;
mod state;
mod tauri_api;

use app::App;
use leptos::mount::mount_to_body;

fn main() {
    console_error_panic_hook::set_once();
    mount_to_body(App);
}
