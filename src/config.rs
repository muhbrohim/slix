use std::env;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

use directories::ProjectDirs;
use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};

pub const CONFIG_HOME_ENV: &str = "SLIX_CONFIG_HOME";

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct GlobalConfig {
    pub active_project: Option<String>,
    pub default_output: Option<String>,
    pub editor: Option<String>,
}

pub fn config_root() -> Result<PathBuf> {
    if let Some(root) = env::var_os(CONFIG_HOME_ENV) {
        return Ok(PathBuf::from(root));
    }

    ProjectDirs::from("", "", "slix")
        .map(|dirs| dirs.config_dir().to_path_buf())
        .ok_or_else(|| Error::Message("cannot determine the Slix configuration directory".into()))
}

pub fn load_global_config() -> Result<GlobalConfig> {
    load_global_config_from(&config_root()?)
}

pub fn load_global_config_from(root: &Path) -> Result<GlobalConfig> {
    let path = root.join("config.toml");
    let contents = match fs::read_to_string(&path) {
        Ok(contents) => contents,
        Err(source) if source.kind() == std::io::ErrorKind::NotFound => {
            return Ok(GlobalConfig::default());
        }
        Err(source) => return Err(Error::Read { path, source }),
    };

    toml::from_str(&contents).map_err(|source| Error::Toml { path, source })
}

pub fn save_global_config(config: &GlobalConfig) -> Result<()> {
    save_global_config_to(&config_root()?, config)
}

pub fn save_global_config_to(root: &Path, config: &GlobalConfig) -> Result<()> {
    let contents = toml::to_string_pretty(config).map_err(|error| {
        Error::Message(format!("cannot serialize global configuration: {error}"))
    })?;
    atomic_write(&root.join("config.toml"), contents.as_bytes())
}

pub(crate) fn atomic_write(path: &Path, contents: &[u8]) -> Result<()> {
    let parent = path
        .parent()
        .ok_or_else(|| Error::Message(format!("{} has no parent directory", path.display())))?;
    fs::create_dir_all(parent).map_err(|source| Error::Write {
        path: parent.to_path_buf(),
        source,
    })?;

    let mut attempt = 0_u32;
    let (temporary, mut file) = loop {
        let candidate = parent.join(format!(
            ".{}.{}.{}.tmp",
            path.file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("slix"),
            std::process::id(),
            attempt
        ));
        match OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&candidate)
        {
            Ok(file) => break (candidate, file),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                attempt = attempt.checked_add(1).ok_or_else(|| Error::Write {
                    path: candidate,
                    source: error,
                })?;
            }
            Err(source) => {
                return Err(Error::Write {
                    path: candidate,
                    source,
                });
            }
        }
    };

    let result = (|| {
        file.write_all(contents).map_err(|source| Error::Write {
            path: temporary.clone(),
            source,
        })?;
        file.sync_all().map_err(|source| Error::Write {
            path: temporary.clone(),
            source,
        })?;
        drop(file);

        replace_file(&temporary, path).map_err(|source| Error::Write {
            path: path.to_path_buf(),
            source,
        })
    })();

    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}

#[cfg(not(windows))]
fn replace_file(source: &Path, destination: &Path) -> std::io::Result<()> {
    fs::rename(source, destination)
}

#[cfg(windows)]
fn replace_file(source: &Path, destination: &Path) -> std::io::Result<()> {
    use std::os::windows::ffi::OsStrExt;

    const MOVEFILE_REPLACE_EXISTING: u32 = 0x1;
    const MOVEFILE_WRITE_THROUGH: u32 = 0x8;

    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn MoveFileExW(
            existing_file_name: *const u16,
            new_file_name: *const u16,
            flags: u32,
        ) -> i32;
    }

    let source: Vec<_> = source.as_os_str().encode_wide().chain(Some(0)).collect();
    let destination: Vec<_> = destination
        .as_os_str()
        .encode_wide()
        .chain(Some(0))
        .collect();
    // SAFETY: Both pointers reference null-terminated UTF-16 buffers for this call.
    let replaced = unsafe {
        MoveFileExW(
            source.as_ptr(),
            destination.as_ptr(),
            MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
        )
    };
    if replaced == 0 {
        Err(std::io::Error::last_os_error())
    } else {
        Ok(())
    }
}
