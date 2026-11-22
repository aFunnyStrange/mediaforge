#![cfg_attr(windows, windows_subsystem = "windows")]
#[cfg(windows)]
mod gui;
fn main() {
    #[cfg(windows)]
    gui::run();
    #[cfg(not(windows))]
    eprintln!("MediaForge currently supports Windows only.");
}
