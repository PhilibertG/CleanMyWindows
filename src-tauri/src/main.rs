// Pas de console supplémentaire en version publiée.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    cmw_lib::run();
}
