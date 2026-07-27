// Keep the console window from appearing alongside the app on Windows release
// builds; debug builds keep it so `println!` diagnostics stay visible.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    stackvo_desktop_lib::run()
}
