use inotify::{EventMask, Inotify, WatchMask};
use nix::unistd::Uid;
use serde::Deserialize;
use std::collections::HashMap;
use std::env;
use std::fs::{self, set_permissions};
use std::os::unix::fs::{chown, PermissionsExt};
use std::path::{Path, PathBuf};
use std::process::exit;

// Define the structure of the config file
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
    // Check if the program is run as root
    if !Uid::effective().is_root() {
        eprintln!("This program must be run as root");
        exit(1);
    }

    // Read config file path from command line
    let args: Vec<String> = env::args().collect();
    let config_path = &args[1];

    // Parse config file
    let config = fs::read_to_string(config_path).expect("Failed to read config file");
    let parsed_config: Config = toml::from_str(&config).unwrap();
    let perm_mappings = parsed_config.perm_mapping;

    // Create a hashmap to store watch descriptors and their corresponding paths
    let mut watches: HashMap<i32, PathBuf> = HashMap::new();

    // Initialize inotify instance
    let mut inotify = Inotify::init().expect("Error while initializing inotify instance");
    let mut buffer = [0; 1024];

    // Add watches for configured directories
    for path in perm_mappings.iter().map(|m| &m.path) {
        if path.as_path().exists() {
            if path.is_dir() {
                if let Some(perm) = map_permission(&perm_mappings, path) {
                    // add configured dir
                    add_watch(&mut inotify, path, &mut watches);
                    chown_and_chmod(perm, path, true);

                    // find additional dirs
                    crawl_path(&mut inotify, path, &mut watches, perm);
                }
            } else if path.is_file() {
                eprintln!(
                    "Failed to add watch for: {}: not a directory, exiting",
                    path.display()
                );
                exit(1);
            }
        } else {
            eprintln!(
                "Failed to add watch for: {}: no such file or directory, exiting",
                path.display()
            );
            exit(1);
        }
    }

    // Start event loop
    loop {
        // Check if there are any watches left
        if watches.is_empty() {
            eprintln!("No watches left, exiting");
            break;
        }

        // Read events from inotify
        let events = inotify
            .read_events_blocking(&mut buffer)
            .expect("Error while reading events");

        // Process events
        for event in events {
            // Get the id and path for the watch descriptor
            let wd_id = event.wd.get_watch_descriptor_id();
            let p = watches.get(&wd_id).unwrap();
            let path = PathBuf::from(p).join(event.name.unwrap_or_default());

            // Process events depending on the event mask
            if event.mask.contains(EventMask::CREATE) || event.mask.contains(EventMask::MOVED_TO) {
                // Handle file or directory creation and moved to events
                if let Some(perm) = map_permission(&perm_mappings, &path) {
                    if event.mask.contains(EventMask::ISDIR) {
                        println!("Directory created: {}", path.display());
                        add_watch(&mut inotify, &path, &mut watches);
                        chown_and_chmod(perm, &path, true);
                        crawl_path(&mut inotify, &path, &mut watches, perm);
                    } else {
                        println!("File created: {}", path.display());
                        chown_and_chmod(perm, &path, false);
                    }
                }
            } else if event.mask.contains(EventMask::DELETE) {
                // Handle file or directory deletion
                if event.mask.contains(EventMask::ISDIR) {
                    println!("Directory deleted: {}", path.display());
                } else {
                    println!("File deleted: {}", path.display());
                }
            } else if event.mask.contains(EventMask::IGNORED) {
                // Handle watch descriptor removal
                println!("Removing watch {} for: {}", wd_id, path.display());
                watches.remove(&wd_id);
            }
        }
    }
}

// Add a watch for a given path and store the watch descriptor and path in a hashmap
fn add_watch(inotify: &mut Inotify, path: &PathBuf, watches: &mut HashMap<i32, PathBuf>) {
    // Add watch for the path
    let new_watch = inotify
        .watches()
        .add(
            path,
            WatchMask::CREATE | WatchMask::DELETE | WatchMask::MOVED_TO,
        )
        .expect("Failed to add file watch");
    let wd_id = new_watch.get_watch_descriptor_id();

    // Store the watch descriptor and path in the hashmap
    println!("Adding watch {} for: {}", wd_id, path.display());
    watches.insert(wd_id, path.to_path_buf());
}

// Find the permission mapping for a given path
fn map_permission<'a>(perm_mappings: &'a Vec<PermMapping>, path: &Path) -> Option<&'a PermMapping> {
    // Find the first mapping that matches the path
    for mapping in perm_mappings {
        if path.starts_with(&mapping.path) {
            return Some(mapping);
        }
    }

    // If no mapping is found, print an error message and return None
    eprintln!("No mapping found for {}", path.display());
    None
}

// Recursively crawl a directory add watches for all subdirectories and set permissions
fn crawl_path(
    inotify: &mut Inotify,
    path: &PathBuf,
    watches: &mut HashMap<i32, PathBuf>,
    perm: &PermMapping,
) {
    println!("Crawling {}", path.display());

    // Iterate over the entries in the directory
    for entry in fs::read_dir(path).expect("Failed to read dir") {
        let path = entry.expect("Failed").path();
        // If the entry is a directory, add a watch and crawl it recursively
        if path.is_dir() {
            add_watch(inotify, &path, watches);
            chown_and_chmod(perm, &path, true);
            crawl_path(inotify, &path, watches, perm);
        } else {
            chown_and_chmod(perm, &path, false);
        }
    }
}

// Change owner and permissions of a file or directory
fn chown_and_chmod(perm: &PermMapping, path: &PathBuf, is_dir: bool) {
    // Get the uid, gid and mode from the permission mapping
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

    // Change the owner and permissions of the file or directory and set the permissions
    chown(path, Some(uid), Some(gid)).expect("Failed to change owner");
    set_permissions(path, fs::Permissions::from_mode(mode)).expect("Failed to change permissions");
}

#[cfg(test)]
mod test {
    use super::*;
    use sequential_test::{parallel, sequential};
    use std::{
        fs::{create_dir, create_dir_all, remove_dir, remove_dir_all, remove_file, File},
        os::linux::fs::MetadataExt,
    };

    #[test]
    #[parallel]
    fn add_watch_test() {
        let mut inotify = Inotify::init().unwrap();
        let mut watches: HashMap<i32, PathBuf> = HashMap::new();
        let path = PathBuf::from("/tmp");

        add_watch(&mut inotify, &path, &mut watches);
        assert_eq!(watches.len(), 1);
    }

    #[test]
    #[parallel]
    fn map_permission_test() {
        let perm_mappings = vec![
            PermMapping {
                path: PathBuf::from("/etc"),
                uid: 0,
                gid: 0,
                fmode: 0o644,
                dmode: 0o755,
            },
            PermMapping {
                path: PathBuf::from("/var"),
                uid: 0,
                gid: 0,
                fmode: 0o644,
                dmode: 0o755,
            },
        ];

        let path = PathBuf::from("/etc/hosts");
        let mapping = map_permission(&perm_mappings, &path).unwrap();
        assert_eq!(mapping.path, PathBuf::from("/etc"));
        assert_eq!(mapping.uid, 0);
        assert_eq!(mapping.gid, 0);
        assert_eq!(mapping.fmode, 0o644);
        assert_eq!(mapping.dmode, 0o755);
    }

    #[test]
    #[ignore]
    #[sequential]
    fn chown_and_chmod_test() {
        let perm = PermMapping {
            path: PathBuf::from("/tmp"),
            uid: 1000,
            gid: 1000,
            fmode: 0o644,
            dmode: 0o755,
        };

        let file = PathBuf::from("/tmp/foo.txt");
        File::create(&file).expect("Failed to create file");
        chown_and_chmod(&perm, &file, false);

        let file_meta = fs::metadata(&file).unwrap();
        remove_file(file).expect("Failed to remove file");
        assert_eq!(file_meta.st_mode() & 0o777, 0o644);
        assert_eq!(file_meta.st_uid(), 1000);
        assert_eq!(file_meta.st_gid(), 1000);

        let dir = PathBuf::from("/tmp/foo");
        create_dir(&dir).expect("Failed to create dir");
        chown_and_chmod(&perm, &dir, true);

        let dir_meta = fs::metadata(&dir).unwrap();
        remove_dir(dir).expect("Failed to remove dir");
        assert_eq!(dir_meta.st_mode() & 0o777, 0o755);
        assert_eq!(dir_meta.st_uid(), 1000);
        assert_eq!(dir_meta.st_gid(), 1000);
    }

    #[test]
    #[ignore]
    #[sequential]
    fn crawl_path_test() {
        let path = PathBuf::from("/tmp/foo/bar");
        create_dir_all(&path).expect("Failed to create dir");
        File::create(&path.join("file.txt")).expect("Failed to create file");

        let mut inotify = Inotify::init().unwrap();
        let mut watches: HashMap<i32, PathBuf> = HashMap::new();
        let perm = PermMapping {
            path: PathBuf::from("/tmp/foo"),
            uid: 1000,
            gid: 1000,
            fmode: 0o644,
            dmode: 0o755,
        };

        let path = PathBuf::from("/tmp/foo");
        add_watch(&mut inotify, &path, &mut watches);
        crawl_path(&mut inotify, &path, &mut watches, &perm);

        let watches_len = watches.len();
        remove_dir_all(path).expect("Failed to remove dir");
        assert_eq!(watches_len, 2);
    }
}
