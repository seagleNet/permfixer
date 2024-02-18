use inotify::{EventMask, Inotify, WatchMask};
use serde::Deserialize;
use std::collections::HashMap;
use std::env;
use std::fs::{self, set_permissions};
use std::os::unix::fs::{chown, PermissionsExt};
use std::path::{Path, PathBuf};
use std::process::exit;

#[derive(Deserialize)]
struct PermMapping {
    path: PathBuf,
    uid: u32,
    gid: u32,
    fmode: u32,
    dmode: u32,
}

#[derive(Deserialize)]
struct Config {
    perm_mapping: Vec<PermMapping>,
}

fn main() {
    // Read config file path from command line
    let args: Vec<String> = env::args().collect();
    let config_path = &args[1];

    // Parse config file
    let config = fs::read_to_string(config_path).expect("Failed to read config file");
    let parsed_config: Config = toml::from_str(&config).unwrap();
    let perm_mappings = parsed_config.perm_mapping;

    let mut watches: HashMap<i32, PathBuf> = HashMap::new();

    let mut inotify = Inotify::init().expect("Error while initializing inotify instance");

    // Watch for modify and close events.
    for path in perm_mappings.iter().map(|m| &m.path) {
        let perm = map_permission(&perm_mappings, path);

        // add configured dir
        add_watch(&mut inotify, path, &mut watches);
        chown_and_chmod(perm, path, true);

        // find additional dirs
        crawl_path(&mut inotify, path, &mut watches, perm);
    }

    // Read events that were added with `Watches::add` above.
    let mut buffer = [0; 1024];
    loop {
        let events = inotify
            .read_events_blocking(&mut buffer)
            .expect("Error while reading events");

        for event in events {
            println!("{:?}", event);
            let wd_id = event.wd.get_watch_descriptor_id();
            let p = watches.get(&wd_id).unwrap();
            let path = PathBuf::from(p).join(event.name.unwrap_or_default());

            if event.mask.contains(EventMask::CREATE) || event.mask.contains(EventMask::MOVED_TO) {
                let perm = map_permission(&perm_mappings, &path);

                if event.mask.contains(EventMask::ISDIR) {
                    println!("Directory created: {}", path.display());
                    add_watch(&mut inotify, &path, &mut watches);
                    chown_and_chmod(perm, &path, true);
                    crawl_path(&mut inotify, &path, &mut watches, perm);
                } else {
                    println!("File created: {}", path.display());
                    chown_and_chmod(perm, &path, false);
                }
            } else if event.mask.contains(EventMask::DELETE) {
                if event.mask.contains(EventMask::ISDIR) {
                    println!("Directory deleted: {}", path.display());
                } else {
                    println!("File deleted: {}", path.display());
                }
            } else if event.mask.contains(EventMask::IGNORED) {
                println!("Removing watch {} for: {}", wd_id, path.display());
                watches.remove(&wd_id);
            }
        }
    }
}

fn add_watch(inotify: &mut Inotify, path: &PathBuf, watches: &mut HashMap<i32, PathBuf>) {
    let new_watch = inotify
        .watches()
        .add(
            path,
            WatchMask::CREATE | WatchMask::DELETE | WatchMask::MOVED_TO,
        )
        .expect("Failed to add file watch");
    let wd_id = new_watch.get_watch_descriptor_id();

    println!("Adding watch {} for: {}", wd_id, path.display());
    watches.insert(wd_id, path.to_path_buf());
}

fn map_permission<'a>(perm_mappings: &'a Vec<PermMapping>, path: &Path) -> &'a PermMapping {
    for mapping in perm_mappings {
        if path.starts_with(&mapping.path) {
            return mapping;
        }
    }

    eprintln!("No mapping found for {}", path.display());
    exit(1);
}

fn crawl_path(
    inotify: &mut Inotify,
    path: &PathBuf,
    watches: &mut HashMap<i32, PathBuf>,
    perm: &PermMapping,
) {
    println!("Crawling {}", path.display());

    for entry in fs::read_dir(path).expect("Failed to read dir") {
        let path = entry.expect("Failed").path();
        if path.is_dir() {
            add_watch(inotify, &path, watches);
            chown_and_chmod(perm, &path, true);
            crawl_path(inotify, &path, watches, perm);
        } else {
            chown_and_chmod(perm, &path, false);
        }
    }
}

fn chown_and_chmod(perm: &PermMapping, path: &PathBuf, is_dir: bool) {
    let uid = perm.uid;
    let gid = perm.gid;
    let mode = if is_dir { perm.dmode } else { perm.fmode };

    println!(
        "Changing owner of {} to {}:{} and permissions to {:o}",
        path.display(),
        uid,
        gid,
        mode
    );

    chown(path, Some(uid), Some(gid)).expect("Failed to change owner");
    set_permissions(path, fs::Permissions::from_mode(mode)).expect("Failed to change permissions");
}
