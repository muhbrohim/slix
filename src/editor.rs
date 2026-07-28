use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::config::GlobalConfig;
use crate::{Error, Result};

const LIVE_LUA: &str = include_str!("../nvim/live.lua");

pub fn launch_live(config: &GlobalConfig) -> Result<()> {
    let editor = config
        .editor
        .clone()
        .or_else(|| std::env::var("VISUAL").ok())
        .unwrap_or_else(|| "nvim".to_owned());
    if editor != "nvim" && !editor.ends_with("nvim.exe") && !editor.ends_with("nvim") {
        return Err(Error::Message(
            "live parsing requires Neovim; set editor = \"nvim\"".to_owned(),
        ));
    }

    let script = temporary_script_path();
    fs::write(&script, LIVE_LUA).map_err(|source| Error::Write {
        path: script.clone(),
        source,
    })?;
    let executable = std::env::current_exe().map_err(|error| {
        Error::Message(format!("cannot determine the Slix executable: {error}"))
    })?;
    let status = Command::new(&editor)
        .arg("--cmd")
        .arg(format!(
            "let g:slix_executable = {}",
            vim_quote(&executable)
        ))
        .arg("-S")
        .arg(&script)
        .status()
        .map_err(|error| Error::Message(format!("cannot start {editor}: {error}")));
    let _ = fs::remove_file(&script);
    let status = status?;
    if status.success() {
        Ok(())
    } else {
        Err(Error::Message(format!("Neovim exited with {status}")))
    }
}

pub fn open_path(config: &GlobalConfig, path: &std::path::Path) -> Result<()> {
    let editor = config
        .editor
        .clone()
        .or_else(|| std::env::var("VISUAL").ok())
        .or_else(|| std::env::var("EDITOR").ok())
        .unwrap_or_else(|| "nvim".to_owned());
    let status = Command::new(&editor)
        .arg(path)
        .status()
        .map_err(|error| Error::Message(format!("cannot start {editor}: {error}")))?;
    if status.success() {
        Ok(())
    } else {
        Err(Error::Message(format!("editor exited with {status}")))
    }
}

fn temporary_script_path() -> PathBuf {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    std::env::temp_dir().join(format!("slix-{}-{nonce}.lua", std::process::id()))
}

fn vim_quote(path: &std::path::Path) -> String {
    format!(
        "'{}'",
        path.display()
            .to_string()
            .replace('\\', "/")
            .replace('\'', "''")
    )
}
