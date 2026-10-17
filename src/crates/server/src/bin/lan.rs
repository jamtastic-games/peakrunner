fn main() {
    #[cfg(not(target_arch = "wasm32"))]
    peakrunner_server::run_game_server();
}
