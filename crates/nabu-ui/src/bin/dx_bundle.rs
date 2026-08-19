//! Binary wrapper that lets `dx bundle` build nabu-ui as a Dioxus web app.
//!
//! nabu-ui is a cdylib (loaded via wasm-bindgen's `#[wasm_bindgen(start)]`),
//! but `dx bundle` needs a binary target. This thin wrapper provides one,
//! delegating to the same `dioxus::web::launch::launch_cfg` entry point.

fn main() {
    nabu_ui::start();
}
