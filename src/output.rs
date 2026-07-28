use crate::Result;
use serde_json::{Map, Value};

use crate::parser::{FieldResult, ParseResult, SectionResult};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputFormat {
    Table,
    Json,
    Yaml,
    Raw,
    Csv,
}

pub fn render(result: &ParseResult, format: OutputFormat) -> Result<String> {
    match format {
        OutputFormat::Table => Ok(render_table(result)),
        OutputFormat::Json => render_json(result),
        OutputFormat::Yaml => render_yaml(result),
        OutputFormat::Raw => Ok(render_raw(result)),
        OutputFormat::Csv => Ok(render_csv(result)),
    }
}

pub fn render_table(result: &ParseResult) -> String {
    let path_width = result
        .fields
        .iter()
        .map(|field| field.path.len())
        .max()
        .unwrap_or(4)
        .max(4);
    let mut output = format!("{:<path_width$}  {:>11}  VALUE\n", "PATH", "BYTES");
    output.push_str(&format!(
        "{}  {}  {}\n",
        "-".repeat(path_width),
        "-".repeat(11),
        "-".repeat(5)
    ));
    for field in &result.fields {
        output.push_str(&format!(
            "{:<path_width$}  {:>5}-{:>5}  {}\n",
            field.path,
            field.start,
            field.end,
            visible(&field.value)
        ));
    }
    output
}

pub fn render_json(result: &ParseResult) -> Result<String> {
    Ok(serde_json::to_string_pretty(&clean_json_value(result))?)
}

pub fn clean_json_value(result: &ParseResult) -> Value {
    let metadata = result
        .body
        .as_ref()
        .map(|body| &body.metadata)
        .unwrap_or(&result.header.metadata);
    let mut output = Map::new();
    output.insert(
        "service_code".to_owned(),
        result
            .service
            .code
            .as_deref()
            .map(str::trim)
            .map_or(Value::Null, |value| Value::String(value.to_owned())),
    );
    for key in ["endpoint", "category", "section", "program"] {
        output.insert(
            key.to_owned(),
            metadata_value(metadata.get(key))
                .map_or(Value::Null, |value| Value::String(value.to_owned())),
        );
    }
    output.insert(
        "header".to_owned(),
        section_value(&result.header, "header."),
    );
    output.insert(
        "body".to_owned(),
        result
            .body
            .as_ref()
            .map(|body| section_value(body, "body."))
            .unwrap_or_else(|| Value::Object(Map::new())),
    );
    output.insert(
        "warnings".to_owned(),
        Value::Array(
            result
                .warnings
                .iter()
                .map(|item| Value::String(item.message.clone()))
                .collect(),
        ),
    );
    output.insert(
        "errors".to_owned(),
        Value::Array(
            result
                .errors
                .iter()
                .map(|item| Value::String(item.message.clone()))
                .collect(),
        ),
    );
    output.insert(
        "unparsed_tail".to_owned(),
        result
            .unparsed_tail
            .as_ref()
            .map_or(Value::Null, |tail| Value::String(tail.clone())),
    );
    Value::Object(output)
}

pub fn render_yaml(result: &ParseResult) -> Result<String> {
    Ok(serde_yaml::to_string(result)?)
}

pub fn render_raw(result: &ParseResult) -> String {
    result
        .fields
        .iter()
        .map(|field| format!("{}={}\n", field.path, visible(&field.value)))
        .collect()
}

pub fn render_csv(result: &ParseResult) -> String {
    let mut output = "path,value,start,end,spec_path,spec_line\n".to_owned();
    for field in &result.fields {
        csv_record(&mut output, field);
    }
    output
}

fn csv_record(output: &mut String, field: &FieldResult) {
    let values = [
        field.path.clone(),
        field.value.clone(),
        field.start.to_string(),
        field.end.to_string(),
        field.spec_path.display().to_string(),
        field.spec_line.to_string(),
    ];
    output.push_str(
        &values
            .iter()
            .map(|value| csv_escape(value))
            .collect::<Vec<_>>()
            .join(","),
    );
    output.push('\n');
}

fn csv_escape(value: &str) -> String {
    if value
        .chars()
        .any(|character| matches!(character, ',' | '"' | '\n' | '\r'))
    {
        format!("\"{}\"", value.replace('"', "\"\""))
    } else {
        value.to_owned()
    }
}

fn visible(value: &str) -> String {
    value.replace('\r', "\\r").replace('\n', "\\n")
}

fn metadata_value(value: Option<&String>) -> Option<&str> {
    value
        .map(String::as_str)
        .filter(|value| !matches!(*value, "" | "-" | "<none>"))
}

fn section_value(section: &SectionResult, prefix: &str) -> Value {
    let mut object = Map::new();
    let mut arrays: Map<String, Value> = Map::new();
    for field in &section.fields {
        let path = field.path.strip_prefix(prefix).unwrap_or(&field.path);
        if let Some((group, index, name)) = repeat_path(path) {
            let entries = arrays
                .entry(group.to_owned())
                .or_insert_with(|| Value::Array(Vec::new()))
                .as_array_mut()
                .expect("repeat entry is always an array");
            while entries.len() < index {
                entries.push(Value::Object(Map::new()));
            }
            entries[index - 1]
                .as_object_mut()
                .expect("repeat item is always an object")
                .insert(name.to_owned(), Value::String(field.value.clone()));
        } else {
            object.insert(path.to_owned(), Value::String(field.value.clone()));
        }
    }
    object.extend(arrays);
    Value::Object(object)
}

fn repeat_path(path: &str) -> Option<(&str, usize, &str)> {
    let bracket = path.find('[')?;
    let close = path[bracket + 1..].find(']')? + bracket + 1;
    if path.as_bytes().get(close + 1) != Some(&b'.') {
        return None;
    }
    let index = path[bracket + 1..close].parse().ok()?;
    Some((&path[..bracket], index, &path[close + 2..]))
}
