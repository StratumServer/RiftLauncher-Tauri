// Keeps the console window off a Windows release build. The launcher writes what
// it has to say to its log, not to a terminal nobody asked for.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    riftlauncher_lib::run()
}
