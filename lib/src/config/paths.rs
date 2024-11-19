use std::path::PathBuf;

use directories::BaseDirs;

pub fn home() -> PathBuf {
    BaseDirs::new()
        .expect("Failed to determine users home directory")
        .home_dir()
        .join(".nimiq")
}

pub fn system() -> PathBuf {
    PathBuf::from("/var/lib/nimiq")
}
