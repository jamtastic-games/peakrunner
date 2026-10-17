use std::io::IsTerminal;
use peakrunner_server::{identity, reach, setup};

const USAGE: &str = "peakrunner-server: a PeakRunner match server.

Run it and answer a few questions; the answers are saved to server.json and
later runs start straight away. Players join from the in-game server list
(if you announce) or with the invite link the server prints. Maps and mods
of your own go in the content/ folder beside server.json; players download
them when they join.

  --setup            ask the setup questions again
  --config-dir DIR   keep server.json and the server key in DIR
  --publicip         for an unlisted server: ask the server list once for this
                     computer's public address and show the public invite link
                     (without it, an unlisted server contacts no server list)
  --healthcheck      exit 0 if this server's match is advancing (its status
                     page on this computer; use --config-dir as when starting)
  --check [PATH]     check content before sharing it: a content folder
                     (maps/<key>/, mods/<id>/), one map folder or one mod
                     folder (default: this server's content/ folder); exit 1
                     on problems
  --schema           print every field a mod's class files may hold (armors,
                     weapons, projectiles, blasts, throwables, deployables),
                     with its allowed values, as Markdown

Environment variables (PEAKRUNNER_MATCH_NAME, PEAKRUNNER_MATCH_ROTATION,
PEAKRUNNER_UDP_BIND, PEAKRUNNER_TLS_CERT, ...) override server.json. Without a
terminal or server.json (Docker) the server never asks anything.";

#[tokio::main(worker_threads = 2)]
async fn main() -> std::io::Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    // Modes that name a rules script get it (docs/modding.md, P6).
    peakrunner_script::install();
    // Maps and mods this server adds to the installed ones: content/ beside
    // server.json (maps/<key>/, mods/<id>/). Registered before anything reads
    // the catalog or server.json (which may name its mods).
    if let Some(d) = setup::config_dir(&args, &|k: &str| std::env::var(k).ok()) { peakrunner_core::catalog::add_root(d.join(setup::CONTENT_DIR)); }
    if args.iter().any(|s| s == "--healthcheck") {
        // This server's status page: 8080 in the container, or loopback at
        // its game port's number when it has a server.json (setup::plan).
        let env = |k: &str| std::env::var(k).ok();
        let saved = match setup::config_dir(&args, &env).map(|d| d.join(setup::CONFIG_FILE)) { Some(p) => setup::load(&p)?, None => None };
        let url = setup::health_url(&setup::plan(saved.as_ref(), &env)?);
        let read = || -> std::io::Result<peakrunner_discovery::MatchStatus> {
            serde_json::from_slice(&peakrunner_discovery::http::http_get(&url, false)?)
                .map_err(std::io::Error::other)
        };
        let first = read()?;
        tokio::time::sleep(std::time::Duration::from_millis(250)).await;
        if read()?.tick <= first.tick { return Err(std::io::Error::other("simulation is not advancing")); }
        return Ok(());
    }
    if args.iter().any(|s| s == "--help" || s == "-h") { println!("{USAGE}"); return Ok(()); }
    if args.iter().any(|s| s == "--schema") { print!("{}", peakrunner_core::registry::fields::markdown()); return Ok(()); }
    if let Some(i) = args.iter().position(|a| a == "--check") {
        // No folder: this server's own content/ beside its server.json.
        let own = setup::config_dir(&args, &|k: &str| std::env::var(k).ok()).map(|d| d.join(setup::CONTENT_DIR).display().to_string());
        let path = match args.get(i + 1).filter(|a| !a.starts_with("--")) {
            Some(p) => p.clone(),
            None => own.ok_or_else(|| std::io::Error::other("--check needs a folder"))?,
        };
        let path = &path;
        let root = std::path::Path::new(path);
        let has_maps = root.join("map.json").is_file() || root.join("maps").is_dir();
        let has_mods = root.join("mod.json").is_file() || root.join("mods").is_dir();
        // Nothing recognisable: say so (the map check explains what it expects).
        let report = if has_maps || !has_mods { peakrunner_core::catalog::check(root) } else { Vec::new() };
        let mods = if has_mods { peakrunner_core::mods::check(root) } else { Vec::new() };
        // A mod's rules scripts and hooks must compile as the server will run them.
        let mods: Vec<(String, Result<String, String>)> = mods.into_iter().map(|(folder, result)| {
            let dir = if root.join("mod.json").is_file() { root.to_path_buf() } else { root.join(&folder) };
            let result = result.and_then(|summary| {
                let loaded = peakrunner_core::mods::load(&dir)?;
                for (name, source) in loaded.scripts {
                    peakrunner_script::check(&format!("rules/{name}.rhai"), &source)?;
                }
                for (name, source) in loaded.hooks {
                    peakrunner_script::check(&format!("hooks/{name}.rhai"), &source)?;
                }
                Ok(summary)
            });
            (folder, result)
        }).collect();
        let problems = report.iter().filter(|c| c.result.is_err()).count() + mods.iter().filter(|m| m.1.is_err()).count();
        println!("Checking {path}");
        for c in &report {
            match &c.result {
                Ok((name, fingerprint, modes)) => println!("  {}: ok, \"{name}\" ({}), fingerprint {}", c.folder,
                    modes.iter().map(|m| m.label()).collect::<Vec<_>>().join(", "), &fingerprint[..16]),
                Err(e) => println!("  {}: PROBLEM: {e}", c.folder),
            }
        }
        for (folder, result) in &mods {
            match result {
                Ok(summary) => println!("  {folder}: ok, {summary}"),
                Err(e) => println!("  {folder}: PROBLEM: {e}"),
            }
        }
        println!("{} checked, {problems} with problems.", report.len() + mods.len());
        std::process::exit(if problems == 0 { 0 } else { 1 });
    }
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();
    println!("PeakRunner server {}: a playtest, free software under the GNU GPL v3, provided as is, without warranty (see COPYRIGHT.txt).",
        peakrunner_protocol::GAME_VERSION);
    let env = |k: &str| std::env::var(k).ok();
    let dir = setup::config_dir(&args, &env);
    let path = dir.as_ref().map(|d| d.join(setup::CONFIG_FILE));
    let saved = match &path { Some(p) => setup::load(p)?, None => None };
    let interactive = std::io::stdin().is_terminal() && std::io::stdout().is_terminal();
    // Started by a game client's Host screen, which shows the settings itself
    // and has no terminal to run --setup in.
    let hosted = env("PEAKRUNNER_HOST_KEY").is_some_and(|k| !k.is_empty());
    let asked = args.iter().any(|s| s == "--setup");
    // Questions only with a person at a terminal: never under Docker or a
    // service manager, and not when the environment already configures it.
    let config = if asked || (saved.is_none() && interactive && !setup::env_configured(&env)) {
        if !interactive { return Err(std::io::Error::other("--setup needs a terminal; edit server.json instead")); }
        let path = path.as_ref().ok_or_else(|| std::io::Error::other("no config directory; use --config-dir DIR"))?;
        let config = setup::wizard(&mut std::io::stdin().lock(), &mut std::io::stdout(), &saved.unwrap_or_default())?;
        setup::save(path, &config)?;
        println!("\nSaved {}. Run with --setup to change it.", path.display());
        if let Some(content) = dir.as_ref().and_then(|d| setup::prepare_content(d)) {
            println!("Your own maps and mods go in {} (see README.txt there).", content.display());
        }
        Some(config)
    } else if let Some(config) = saved {
        if !hosted { println!("Using {}; run with --setup to change it.\n{}", path.as_ref().unwrap().display(), setup::summary(&config)); }
        Some(config)
    } else { None };
    let plan = setup::plan(config.as_ref(), &env)?;
    let tls = identity::load(dir.as_deref(), &env)?;
    if config.is_some() && tls.generated && !hosted {
        println!("Server key: sha256={} (in {})", peakrunner_discovery::quic::fingerprint_hex(&tls.fingerprint),
            dir.as_ref().unwrap().display());
    }
    // Hosted servers announce themselves to a public directory: with only
    // the environment, set PEAKRUNNER_DIRECTORY_URL (e.g.
    // https://dir.peakrunner.net/announce) and PEAKRUNNER_ADVERTISE_URL (this
    // server's quic://host:port as players reach it). The directory checks
    // the address and queries our status before listing us; announcing
    // again keeps the listing alive.
    let mapping = if config.is_some() || plan.announce.is_some() {
        let url = plan.announce.as_ref().and_then(|a| a.url.clone()).or(plan.private_url.clone());
        Some(reach::start(reach::Reach {
            directory: plan.announce.as_ref().map(|a| a.directory.clone()),
            url, lookup: plan.public_ip_lookup(args.iter().any(|s| s == "--publicip")),
            port: plan.bind.port(), upnp: plan.upnp,
            pin: tls.generated.then_some(tls.fingerprint),
            talk: config.is_some(),
        }))
    } else { None };
    let result = peakrunner_server::quic::run_match(plan.bind, &tls.cert, &tls.key, plan.settings).await;
    if let Some(mapping) = mapping { mapping.release(); }
    result
}
