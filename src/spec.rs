use std::collections::{BTreeMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::{Error, Result};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Spec {
    pub path: PathBuf,
    pub metadata: BTreeMap<String, String>,
    pub entries: Vec<SpecEntry>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SpecEntry {
    Field(FieldSpec),
    Repeat(RepeatSpec),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FieldSpec {
    pub name: String,
    pub length: usize,
    pub type_hint: Option<String>,
    pub line: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RepeatSpec {
    pub name: String,
    pub count: usize,
    pub fields: Vec<FieldSpec>,
    pub line: usize,
}

impl Spec {
    pub fn parse(source: &str, path: impl Into<PathBuf>) -> Result<Self> {
        parse_spec(source, path)
    }

    pub fn load(path: impl AsRef<Path>) -> Result<Self> {
        load_spec(path)
    }

    pub fn byte_len(&self) -> usize {
        self.entries
            .iter()
            .map(|entry| match entry {
                SpecEntry::Field(field) => field.length,
                SpecEntry::Repeat(group) => {
                    group.fields.iter().map(|field| field.length).sum::<usize>() * group.count
                }
            })
            .sum()
    }
}

pub fn load_spec(path: impl AsRef<Path>) -> Result<Spec> {
    let path = path.as_ref();
    let source = fs::read_to_string(path).map_err(|source| Error::Read {
        path: path.to_owned(),
        source,
    })?;
    parse_spec(&source, path)
}

pub fn parse_spec(source: &str, path: impl Into<PathBuf>) -> Result<Spec> {
    let path = path.into();
    let mut metadata = BTreeMap::new();
    let mut entries = Vec::new();
    let mut names = HashSet::new();
    let mut group: Option<RepeatSpec> = None;
    let mut fields_started = false;

    for (index, raw_line) in source.lines().enumerate() {
        let line_number = index + 1;
        let line = raw_line.trim();
        if line.is_empty() {
            continue;
        }

        if line.starts_with('#') {
            if !fields_started {
                if let Some((key, value)) = line[1..].split_once(':') {
                    let key = key.trim();
                    if !key.is_empty() {
                        metadata.insert(key.to_owned(), value.trim().to_owned());
                    }
                }
            }
            continue;
        }

        let line = line
            .split_once('#')
            .map_or(line, |(content, _)| content)
            .trim();
        if line.is_empty() {
            continue;
        }
        fields_started = true;

        if line.starts_with('@') {
            let parts: Vec<_> = line.split_whitespace().collect();
            match parts.as_slice() {
                ["@repeat", name, count] => {
                    if group.is_some() {
                        return invalid(
                            &path,
                            line_number,
                            "nested @repeat groups are not allowed",
                        );
                    }
                    let count = positive_number(count, &path, line_number, "repeat count")?;
                    if !names.insert((*name).to_owned()) {
                        return invalid(&path, line_number, &format!("duplicate name `{name}`"));
                    }
                    group = Some(RepeatSpec {
                        name: (*name).to_owned(),
                        count,
                        fields: Vec::new(),
                        line: line_number,
                    });
                }
                ["@end"] => {
                    let completed = group.take().ok_or_else(|| {
                        Error::Message(format!(
                            "{}:{line_number}: @end without @repeat",
                            path.display()
                        ))
                    })?;
                    if completed.fields.is_empty() {
                        return invalid(&path, line_number, "repeat group must contain a field");
                    }
                    entries.push(SpecEntry::Repeat(completed));
                }
                _ => return invalid(&path, line_number, "malformed directive"),
            }
            continue;
        }

        let parts: Vec<_> = line.split_whitespace().collect();
        if !(2..=3).contains(&parts.len()) {
            return invalid(
                &path,
                line_number,
                "expected: name positive_length [type_hint]",
            );
        }
        let length = positive_number(parts[1], &path, line_number, "field length")?;
        if let Some(active_group) = group.as_ref() {
            if active_group
                .fields
                .iter()
                .any(|field| field.name == parts[0])
            {
                return invalid(
                    &path,
                    line_number,
                    &format!("duplicate name `{}` in repeat group", parts[0]),
                );
            }
        } else if !names.insert(parts[0].to_owned()) {
            return invalid(
                &path,
                line_number,
                &format!("duplicate name `{}`", parts[0]),
            );
        }
        let field = FieldSpec {
            name: parts[0].to_owned(),
            length,
            type_hint: parts.get(2).map(|value| (*value).to_owned()),
            line: line_number,
        };
        if let Some(group) = group.as_mut() {
            group.fields.push(field);
        } else {
            entries.push(SpecEntry::Field(field));
        }
    }

    if let Some(group) = group {
        return invalid(
            &path,
            group.line,
            &format!("repeat group `{}` is missing @end", group.name),
        );
    }
    if entries.is_empty() {
        return Err(Error::Message(format!(
            "{}: specification must contain a field",
            path.display()
        )));
    }

    Ok(Spec {
        path,
        metadata,
        entries,
    })
}

fn positive_number(value: &str, path: &Path, line: usize, label: &str) -> Result<usize> {
    match value.parse::<usize>() {
        Ok(number) if number > 0 => Ok(number),
        _ => invalid(path, line, &format!("{label} must be a positive integer")),
    }
}

fn invalid<T>(path: &Path, line: usize, message: &str) -> Result<T> {
    Err(Error::Message(format!(
        "{}:{line}: {message}",
        path.display()
    )))
}
