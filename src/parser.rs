use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use encoding_rs::Encoding;
use serde::{Deserialize, Serialize};

use crate::Result;
use crate::spec::{FieldSpec, Spec, SpecEntry};

pub const SCHEMA_VERSION: u8 = 1;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ParseResult {
    pub schema_version: u8,
    pub service: ServiceMetadata,
    pub header: SectionResult,
    pub body: Option<SectionResult>,
    pub fields: Vec<FieldResult>,
    pub unparsed_tail: Option<String>,
    pub warnings: Vec<Diagnostic>,
    pub errors: Vec<Diagnostic>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ServiceMetadata {
    pub code: Option<String>,
    pub dispatch_code: Option<String>,
    pub spec_path: Option<PathBuf>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SectionResult {
    pub spec_path: PathBuf,
    pub metadata: BTreeMap<String, String>,
    pub fields: Vec<FieldResult>,
    pub groups: Vec<GroupResult>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GroupResult {
    pub name: String,
    pub occurrences: Vec<Vec<FieldResult>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FieldResult {
    pub path: String,
    pub value: String,
    pub start: usize,
    pub end: usize,
    pub spec_path: PathBuf,
    pub spec_line: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Diagnostic {
    pub code: String,
    pub message: String,
}

pub fn parse_message(
    bytes: &[u8],
    specs_root: impl AsRef<Path>,
    encoding: &'static Encoding,
) -> Result<ParseResult> {
    let root = specs_root.as_ref();
    let header_spec = Spec::load(root.join("header.spec"))?;
    parse_with_header(bytes, root, encoding, header_spec)
}

pub fn parse(
    bytes: &[u8],
    specs_root: impl AsRef<Path>,
    encoding: &'static Encoding,
) -> Result<ParseResult> {
    parse_message(bytes, specs_root, encoding)
}

fn parse_with_header(
    bytes: &[u8],
    root: &Path,
    encoding: &'static Encoding,
    header_spec: Spec,
) -> Result<ParseResult> {
    let mut offset = 0;
    let mut warnings = Vec::new();
    let mut errors = Vec::new();
    let header = parse_section(
        &header_spec,
        "header",
        bytes,
        &mut offset,
        encoding,
        &mut warnings,
        &mut errors,
    );
    let service_code = header
        .fields
        .iter()
        .find(|field| field.path == "header.service_code")
        .map(|field| field.value.clone());
    let dispatch_code = service_code
        .as_deref()
        .map(str::trim)
        .filter(|code| !code.is_empty())
        .map(str::to_owned);

    let body_path = dispatch_code
        .as_deref()
        .and_then(|code| find_body_spec(root, code));
    let body = if let Some(path) = body_path.as_ref() {
        let spec = Spec::load(path)?;
        Some(parse_section(
            &spec,
            "body",
            bytes,
            &mut offset,
            encoding,
            &mut warnings,
            &mut errors,
        ))
    } else {
        warnings.push(Diagnostic::new(
            "missing_body_spec",
            match dispatch_code.as_deref() {
                Some(code) => format!("body specification for service `{code}` was not found"),
                None => "header did not produce a service_code".to_owned(),
            },
        ));
        None
    };

    let unparsed_tail = if offset < bytes.len() {
        warnings.push(Diagnostic::new(
            "overflow",
            format!(
                "{} byte(s) remain after the selected specifications",
                bytes.len() - offset
            ),
        ));
        Some(decode(encoding, &bytes[offset..], &mut warnings))
    } else {
        None
    };
    let mut fields = header.fields.clone();
    if let Some(body) = &body {
        fields.extend(body.fields.clone());
    }

    Ok(ParseResult {
        schema_version: SCHEMA_VERSION,
        service: ServiceMetadata {
            code: service_code,
            dispatch_code,
            spec_path: body_path,
        },
        header,
        body,
        fields,
        unparsed_tail,
        warnings,
        errors,
    })
}

fn find_body_spec(root: &Path, service_code: &str) -> Option<PathBuf> {
    let code = service_code.strip_prefix('#').unwrap_or(service_code);
    [format!("{code}.spec"), format!("#{code}.spec")]
        .into_iter()
        .map(|name| root.join("body").join(name))
        .find(|path| path.is_file())
}

fn parse_section(
    spec: &Spec,
    prefix: &str,
    bytes: &[u8],
    offset: &mut usize,
    encoding: &'static Encoding,
    warnings: &mut Vec<Diagnostic>,
    errors: &mut Vec<Diagnostic>,
) -> SectionResult {
    let mut fields = Vec::new();
    let mut groups = Vec::new();
    let mut truncated = false;

    for entry in &spec.entries {
        if truncated {
            break;
        }
        match entry {
            SpecEntry::Field(field) => {
                let path = format!("{prefix}.{}", field.name);
                if let Some(value) =
                    parse_field(field, path, spec, bytes, offset, encoding, warnings, errors)
                {
                    fields.push(value);
                } else {
                    truncated = true;
                }
            }
            SpecEntry::Repeat(group) => {
                let mut occurrences = Vec::new();
                for index in 0..group.count {
                    let mut occurrence = Vec::new();
                    for field in &group.fields {
                        let path =
                            format!("{prefix}.{}[{:02}].{}", group.name, index + 1, field.name);
                        if let Some(value) = parse_field(
                            field, path, spec, bytes, offset, encoding, warnings, errors,
                        ) {
                            fields.push(value.clone());
                            occurrence.push(value);
                        } else {
                            truncated = true;
                            break;
                        }
                    }
                    if !occurrence.is_empty() {
                        occurrences.push(occurrence);
                    }
                    if truncated {
                        break;
                    }
                }
                groups.push(GroupResult {
                    name: group.name.clone(),
                    occurrences,
                });
            }
        }
    }

    SectionResult {
        spec_path: spec.path.clone(),
        metadata: spec.metadata.clone(),
        fields,
        groups,
    }
}

fn parse_field(
    field: &FieldSpec,
    path: String,
    spec: &Spec,
    bytes: &[u8],
    offset: &mut usize,
    encoding: &'static Encoding,
    warnings: &mut Vec<Diagnostic>,
    errors: &mut Vec<Diagnostic>,
) -> Option<FieldResult> {
    let available = bytes.len().saturating_sub(*offset);
    if available < field.length {
        errors.push(Diagnostic::new(
            "truncated_field",
            format!(
                "field `{path}` requires {} byte(s), but only {available} remain",
                field.length
            ),
        ));
        return None;
    }
    let start = *offset;
    *offset += field.length;
    let value = decode(encoding, &bytes[start..*offset], warnings);
    Some(FieldResult {
        path,
        value,
        start,
        end: *offset - 1,
        spec_path: spec.path.clone(),
        spec_line: field.line,
    })
}

fn decode(encoding: &'static Encoding, bytes: &[u8], warnings: &mut Vec<Diagnostic>) -> String {
    let (value, malformed) = encoding.decode_without_bom_handling(bytes);
    if malformed {
        warnings.push(Diagnostic::new(
            "malformed_encoding",
            format!("input contains bytes invalid for {}", encoding.name()),
        ));
    }
    value.into_owned()
}

impl Diagnostic {
    fn new(code: &str, message: String) -> Self {
        Self {
            code: code.to_owned(),
            message,
        }
    }
}
