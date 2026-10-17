//! PeakRunner's icon in its Windows executable (`assets/icons/peakrunner.ico`, made by
//! `scripts/build-icons.py`), compiled by MinGW's windres as the rest of the
//! Windows cross build is (docs/client-builds.md). Other targets: nothing.
use std::{env, path::PathBuf, process::Command};

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    if env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") { return; }
    if env::var("CARGO_CFG_TARGET_ENV").as_deref() != Ok("gnu") {
        println!("cargo:warning=the Windows icon is built only for MinGW (windows-gnu) targets");
        return;
    }
    let icon = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap()).join("assets/icons/peakrunner.ico");
    println!("cargo:rerun-if-changed={}", icon.display());
    let out = PathBuf::from(env::var("OUT_DIR").unwrap());
    let script = out.join("icon.rc");
    std::fs::write(&script, format!("1 ICON \"{}\"\n", icon.display().to_string().replace('\\', "/"))).unwrap();
    let windres = env::var("WINDRES")
        .unwrap_or_else(|_| format!("{}-w64-mingw32-windres", env::var("CARGO_CFG_TARGET_ARCH").unwrap()));
    let object = out.join("icon.o");
    let status = Command::new(&windres).arg(&script).args(["-O", "coff", "-o"]).arg(&object).status()
        .unwrap_or_else(|e| panic!("{windres}: {e} (the Windows build needs MinGW's windres)"));
    assert!(status.success(), "{windres} failed on {}", script.display());
    println!("cargo:rustc-link-arg-bins={}", object.display());
}
