use std::env;
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::config::{atomic_write, config_root, load_global_config_from, save_global_config_to};
use crate::error::{Error, Result};

pub const PROJECT_ENV: &str = "SLIX_PROJECT";

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ProjectConfig {
    pub name: String,
    #[serde(default = "default_encoding")]
    pub encoding: String,
    #[serde(default = "default_specs")]
    pub specs: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Project {
    pub root: PathBuf,
    pub config: ProjectConfig,
}

fn default_encoding() -> String {
    "utf-8".into()
}

fn default_specs() -> String {
    "specs".into()
}

impl ProjectConfig {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            encoding: default_encoding(),
            specs: default_specs(),
        }
    }
}

impl Project {
    pub fn specs_dir(&self) -> PathBuf {
        self.root.join(&self.config.specs)
    }
}

pub fn validate_project_name(name: &str) -> Result<()> {
    if !name.is_empty()
        && name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
    {
        return Ok(());
    }

    Err(Error::Message(format!(
        "invalid project name {name:?}; use only ASCII letters, digits, '_' or '-'"
    )))
}

pub fn list_projects() -> Result<Vec<Project>> {
    list_projects_from(&config_root()?)
}

pub fn list_projects_from(root: &Path) -> Result<Vec<Project>> {
    let projects_dir = root.join("projects");
    let entries = match fs::read_dir(&projects_dir) {
        Ok(entries) => entries,
        Err(source) if source.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(source) => {
            return Err(Error::Read {
                path: projects_dir,
                source,
            });
        }
    };
    let mut projects = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|source| Error::Read {
            path: projects_dir.clone(),
            source,
        })?;
        if entry.file_name().to_string_lossy().starts_with('.') {
            continue;
        }
        let file_type = entry.file_type().map_err(|source| Error::Read {
            path: entry.path(),
            source,
        })?;
        if file_type.is_dir() && !file_type.is_symlink() {
            projects.push(load_project(entry.path())?);
        }
    }
    projects.sort_by(|left, right| left.config.name.cmp(&right.config.name));
    Ok(projects)
}

pub fn get_project(name: &str) -> Result<Project> {
    get_project_from(&config_root()?, name)
}

pub fn get_project_from(root: &Path, name: &str) -> Result<Project> {
    validate_project_name(name)?;
    load_project(root.join("projects").join(name))
}

pub fn resolve_active_project(explicit: Option<&str>) -> Result<Project> {
    resolve_active_project_from(&config_root()?, explicit)
}

pub fn resolve_active_project_from(root: &Path, explicit: Option<&str>) -> Result<Project> {
    let name = explicit
        .filter(|name| !name.is_empty())
        .map(str::to_owned)
        .or_else(|| env::var(PROJECT_ENV).ok().filter(|name| !name.is_empty()))
        .map(Ok)
        .unwrap_or_else(|| {
            load_global_config_from(root)?
                .active_project
                .ok_or_else(|| Error::Message("no active project is configured".into()))
        })?;
    get_project_from(root, &name)
}

pub fn set_active_project(name: &str) -> Result<()> {
    set_active_project_from(&config_root()?, name)
}

pub fn set_active_project_from(root: &Path, name: &str) -> Result<()> {
    get_project_from(root, name)?;
    let mut config = load_global_config_from(root)?;
    config.active_project = Some(name.to_owned());
    save_global_config_to(root, &config)
}

pub fn import_project(name: &str, source_specs: &Path) -> Result<Project> {
    import_project_from(&config_root()?, name, source_specs)
}

pub fn import_project_from(root: &Path, name: &str, source_specs: &Path) -> Result<Project> {
    validate_project_name(name)?;
    let source_type = fs::symlink_metadata(source_specs).map_err(|source| Error::Read {
        path: source_specs.to_path_buf(),
        source,
    })?;
    if source_type.file_type().is_symlink() || !source_type.is_dir() {
        return Err(Error::Message(format!(
            "spec source {} must be a directory and not a symlink",
            source_specs.display()
        )));
    }

    let projects_dir = root.join("projects");
    fs::create_dir_all(&projects_dir).map_err(|source| Error::Write {
        path: projects_dir.clone(),
        source,
    })?;
    let destination = projects_dir.join(name);
    if destination.exists() {
        return Err(Error::Message(format!("project {name:?} already exists")));
    }

    let staging = staging_path(&projects_dir, name)?;
    fs::create_dir(&staging).map_err(|source| Error::Write {
        path: staging.clone(),
        source,
    })?;
    let result = (|| {
        let specs = staging.join("specs");
        fs::create_dir(&specs).map_err(|source| Error::Write {
            path: specs.clone(),
            source,
        })?;
        for item in ["header.spec", "body", "reff"] {
            let source = source_specs.join(item);
            match fs::symlink_metadata(&source) {
                Ok(_) => copy_without_symlinks(&source, &specs.join(item))?,
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(source_error) => {
                    return Err(Error::Read {
                        path: source,
                        source: source_error,
                    });
                }
            }
        }

        let project_config = ProjectConfig::new(name);
        let contents = toml::to_string_pretty(&project_config)
            .map_err(|error| Error::Message(format!("cannot serialize project: {error}")))?;
        atomic_write(&staging.join("project.toml"), contents.as_bytes())?;
        fs::rename(&staging, &destination).map_err(|source| Error::Write {
            path: destination.clone(),
            source,
        })?;
        Ok(Project {
            root: destination,
            config: project_config,
        })
    })();

    if result.is_err() {
        let _ = fs::remove_dir_all(&staging);
    }
    result
}

fn load_project(project_root: PathBuf) -> Result<Project> {
    let path = project_root.join("project.toml");
    let contents = fs::read_to_string(&path).map_err(|source| Error::Read {
        path: path.clone(),
        source,
    })?;
    let config = toml::from_str(&contents).map_err(|source| Error::Toml { path, source })?;
    Ok(Project {
        root: project_root,
        config,
    })
}

fn staging_path(projects_dir: &Path, name: &str) -> Result<PathBuf> {
    for attempt in 0..u32::MAX {
        let path = projects_dir.join(format!(".{name}.import.{}.{}", std::process::id(), attempt));
        if !path.exists() {
            return Ok(path);
        }
    }
    Err(Error::Message(format!(
        "cannot allocate a staging directory for project {name:?}"
    )))
}

fn copy_without_symlinks(source: &Path, destination: &Path) -> Result<()> {
    let metadata = fs::symlink_metadata(source).map_err(|source_error| Error::Read {
        path: source.to_path_buf(),
        source: source_error,
    })?;
    if metadata.file_type().is_symlink() {
        return Err(Error::Message(format!(
            "refusing to import symlink {}",
            source.display()
        )));
    }
    if metadata.is_file() {
        fs::copy(source, destination).map_err(|source_error| Error::Write {
            path: destination.to_path_buf(),
            source: source_error,
        })?;
        return Ok(());
    }
    if !metadata.is_dir() {
        return Err(Error::Message(format!(
            "unsupported file type in import: {}",
            source.display()
        )));
    }

    fs::create_dir(destination).map_err(|source_error| Error::Write {
        path: destination.to_path_buf(),
        source: source_error,
    })?;
    let entries = fs::read_dir(source).map_err(|source_error| Error::Read {
        path: source.to_path_buf(),
        source: source_error,
    })?;
    for entry in entries {
        let entry = entry.map_err(|source_error| Error::Read {
            path: source.to_path_buf(),
            source: source_error,
        })?;
        copy_without_symlinks(&entry.path(), &destination.join(entry.file_name()))?;
    }
    Ok(())
}
