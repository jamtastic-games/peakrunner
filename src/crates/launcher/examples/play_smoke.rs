//! Launch an actual signed installed client and verify its independent game lock.
use peakrunner_launcher::{self as updater, Result, Store};
fn main() -> Result<()> {
    std::env::var("PEAKRUNNER_LAUNCHER_DATA_DIR").expect("Set an isolated QA store");
    let store = Store::open(
        updater::data_dir()?,
        updater::public_key()?,
        updater::platform().into(),
    )?;
    let mut child = store.play()?;
    std::thread::sleep(std::time::Duration::from_secs(8));
    let running = child.try_wait()?.is_none();
    let locked = store.game_idle().is_err();
    if running {
        child.kill()?;
        child.wait()?;
    }
    assert!(
        running,
        "Managed client exited unexpectedly; inspect its game.log"
    );
    assert!(locked, "Running game did not hold its update lock");
    assert!(
        store.game_idle().is_ok(),
        "Game lock was not released on exit"
    );
    println!("PASS: signed client launched, held its own lock, and released it on exit");
    Ok(())
}
