use crate::Result;
use crate::parser::{FieldResult, ParseResult};

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
    Ok(serde_json::to_string_pretty(result)?)
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
