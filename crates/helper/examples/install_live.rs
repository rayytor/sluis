//! Runs a real install, update and uninstall against GitHub's latest
//! zapret release in a scratch directory, without systemd (for a
//! container): `cargo run -p sluis-helper --example install_live -- DIR`.

use std::path::PathBuf;

use sluis_helper::install::{self, Paths, Real};

fn main() {
    let root = PathBuf::from(std::env::args().nth(1).expect("a scratch directory"));
    std::fs::create_dir_all(root.join("opt")).unwrap();
    let paths = Paths {
        dir: root.join("opt/zapret"),
        unit: root.join("zapret.service"),
        system_unit: root.join("no-system-unit"),
    };
    let step = |s: install::Step| println!("  step: {}", s.as_str());
    println!("kind before: {:?}", install::kind(&paths));

    println!("install:");
    let started = std::time::Instant::now();
    println!(
        "  -> {:?} in {:.1?}",
        install::install(&Real, &paths, &step),
        started.elapsed()
    );
    println!("kind: {:?}", install::kind(&paths));
    let nfqws = paths.dir.join("nfq/nfqws");
    println!(
        "nfqws: {} ({:?})",
        nfqws.display(),
        std::fs::metadata(&nfqws).map(|m| m.len())
    );
    let config = std::fs::read_to_string(paths.dir.join("config")).unwrap_or_default();
    for key in ["FWTYPE=", "MODE_FILTER=", "NFQWS_ENABLE="] {
        println!(
            "  {}",
            config
                .lines()
                .find(|l| l.starts_with(key))
                .unwrap_or("(missing)")
        );
    }
    println!("unit written: {}", paths.unit.exists());
    println!(
        "again: {:?}",
        install::install(&Real, &paths, &step).map_err(|e| e.to_string())
    );

    println!(
        "update when current: {:?}",
        install::update(&Real, &paths, &step).map_err(|e| e.to_string())
    );
    // pretend an older release is installed, with the user's own settings
    std::fs::write(paths.dir.join(".sluis"), "72.8").unwrap();
    std::fs::write(
        paths.dir.join("config"),
        config.replace("MODE_FILTER=none", "MODE_FILTER=none\n# mine"),
    )
    .unwrap();
    std::fs::write(
        paths.dir.join("ipset/zapret-hosts-user.txt"),
        "discord.com\n",
    )
    .unwrap();
    println!("update from 72.8:");
    println!("  -> {:?}", install::update(&Real, &paths, &step));
    println!("kind: {:?}", install::kind(&paths));
    println!(
        "config kept: {}, list kept: {:?}",
        std::fs::read_to_string(paths.dir.join("config"))
            .unwrap()
            .contains("# mine"),
        std::fs::read_to_string(paths.dir.join("ipset/zapret-hosts-user.txt")).ok()
    );

    println!(
        "uninstall: {:?}",
        install::uninstall(&Real, &paths).map_err(|e| e.to_string())
    );
    println!(
        "kind after: {:?}, dir left: {}",
        install::kind(&paths),
        paths.dir.exists()
    );
    let leftovers: Vec<_> = std::fs::read_dir(root.join("opt"))
        .unwrap()
        .map(|e| e.unwrap().file_name())
        .collect();
    println!("left in opt: {leftovers:?}");
}
