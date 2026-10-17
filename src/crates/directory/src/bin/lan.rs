fn main() {
    #[cfg(not(target_arch = "wasm32"))]
    native();
}

#[cfg(not(target_arch = "wasm32"))]
fn native() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();
    let bind = std::env::args().nth(1).unwrap_or_else(|| "127.0.0.1:7780".into());
    let directory = match peakrunner_directory::lan::serve(&bind) {
        Ok(directory) => directory,
        Err(err) => {
            eprintln!("directory failed: {err}");
            std::process::exit(1);
        }
    };
    loop {
        std::thread::park();
        let _ = &directory;
    }
}
