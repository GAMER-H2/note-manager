// Prevents an additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    // All application logic lives in the library crate (`lib.rs`) so that the
    // desktop binary and the Android/iOS builds share the exact same commands
    // and setup. On mobile, Tauri loads `note_manager_lib` (the cdylib) and
    // calls `run()` directly via the `mobile_entry_point`; here we call it too.
    note_manager_lib::run();
}
