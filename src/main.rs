use std::fs;
use std::io::{self, IsTerminal, Read};
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode, Stdio};

use clap::{Args, Parser, Subcommand, ValueEnum};
use encoding_rs::Encoding;
use serde::Serialize;
use slix::config::{config_root, load_global_config};
use slix::editor;
use slix::output::{self, OutputFormat};
use slix::parser::{ParseResult, parse_message};
use slix::project::{import_project, list_projects, resolve_active_project, set_active_project};
use slix::spec::Spec;
use slix::{Error, Result};

#[derive(Parser)]
#[command(version, about)]
struct Cli {
    /// Use a project instead of the configured active project
    #[arg(long, global = true)]
    project: Option<String>,

    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    /// Parse messages from a file, standard input, or the clipboard
    Parse(ParseArgs),
    /// List available specifications
    List(ListArgs),
    /// Open a specification or the active specification directory
    Open {
        /// Service code of the specification to open
        service_code: Option<String>,
    },
    /// Inspect and manage specification projects
    Project {
        #[command(subcommand)]
        command: Option<ProjectCommand>,
    },
    /// Check configuration, project specifications, and Neovim
    Doctor,
    /// Show statistics for the active project
    Stats,
}

#[derive(Args)]
struct ParseArgs {
    /// File containing one message per non-empty line; reads stdin when omitted
    file: Option<PathBuf>,

    /// Read one message from the system clipboard
    #[arg(long, conflicts_with_all = ["file", "message"])]
    clipboard: bool,

    /// Parse a message supplied directly on the command line
    #[arg(long, conflicts_with_all = ["file", "clipboard"])]
    message: Option<String>,

    /// Output representation
    #[arg(long, value_enum)]
    output: Option<Format>,

    #[arg(long, hide = true)]
    protocol: bool,
}

#[derive(Args)]
struct ListArgs {
    /// Filter by service code, path, or metadata
    query: Option<String>,

    /// Emit machine-readable JSON
    #[arg(long)]
    json: bool,
}

#[derive(Subcommand)]
enum ProjectCommand {
    /// List imported projects
    List,
    /// Select the active project
    Use { name: String },
    /// Copy a Slicer-compatible specification directory into Slix
    Import {
        name: String,
        specs_directory: PathBuf,
    },
}

#[derive(Clone, Copy, ValueEnum)]
enum Format {
    Table,
    Json,
    Yaml,
    Csv,
    Raw,
}

#[derive(Serialize)]
struct BatchResult {
    schema_version: u8,
    results: Vec<ParseResult>,
    rendered_json: Vec<String>,
}

#[derive(Serialize)]
struct SpecRow {
    service_code: String,
    spec_path: PathBuf,
    fields: usize,
    bytes: usize,
    endpoint: Option<String>,
    category: Option<String>,
    section: Option<String>,
    program: Option<String>,
}

fn main() -> ExitCode {
    match run() {
        Ok(code) => ExitCode::from(code),
        Err(error) => {
            eprintln!("slix: {error}");
            ExitCode::from(2)
        }
    }
}

fn run() -> Result<u8> {
    let cli = Cli::parse();
    match cli.command {
        None if !io::stdin().is_terminal() => parse_command(
            cli.project.as_deref(),
            ParseArgs {
                file: None,
                clipboard: false,
                message: None,
                output: None,
                protocol: false,
            },
        ),
        None => {
            editor::launch_live(&load_global_config()?)?;
            Ok(0)
        }
        Some(Commands::Parse(args)) => parse_command(cli.project.as_deref(), args),
        Some(Commands::List(args)) => list_command(cli.project.as_deref(), args),
        Some(Commands::Open { service_code }) => {
            let project = resolve_active_project(cli.project.as_deref())?;
            let path = match service_code {
                Some(code) => find_spec(&project.specs_dir(), &code).ok_or_else(|| {
                    Error::Message(format!("specification for `{code}` was not found"))
                })?,
                None => project.specs_dir(),
            };
            editor::open_path(&load_global_config()?, &path)?;
            Ok(0)
        }
        Some(Commands::Project { command }) => project_command(command),
        Some(Commands::Doctor) => doctor(cli.project.as_deref()),
        Some(Commands::Stats) => stats(cli.project.as_deref()),
    }
}

fn parse_command(project_name: Option<&str>, args: ParseArgs) -> Result<u8> {
    let project = resolve_active_project(project_name)?;
    let encoding = Encoding::for_label(project.config.encoding.as_bytes()).ok_or_else(|| {
        Error::Message(format!(
            "unsupported encoding `{}`",
            project.config.encoding
        ))
    })?;
    let input = read_input(&args)?;
    let lines = split_nonempty_lines(&input);
    if lines.is_empty() {
        return Err(Error::Message(
            "no non-empty messages were provided".to_owned(),
        ));
    }
    let mut results = Vec::with_capacity(lines.len());
    for line in lines {
        results.push(parse_message(line, project.specs_dir(), encoding)?);
    }
    let format = args
        .output
        .or_else(|| {
            load_global_config()
                .ok()
                .and_then(|config| config.default_output)
                .and_then(|value| Format::from_str(&value, true).ok())
        })
        .unwrap_or(Format::Json);
    print_results(&results, format, args.protocol)?;
    Ok(if results.iter().any(|result| !result.errors.is_empty()) {
        1
    } else {
        0
    })
}

fn read_input(args: &ParseArgs) -> Result<Vec<u8>> {
    if let Some(message) = &args.message {
        return Ok(message.as_bytes().to_vec());
    }
    if let Some(path) = &args.file {
        return fs::read(path).map_err(|source| Error::Read {
            path: path.clone(),
            source,
        });
    }
    if args.clipboard {
        return read_clipboard();
    }
    let mut input = Vec::new();
    io::stdin()
        .read_to_end(&mut input)
        .map_err(|error| Error::Message(format!("cannot read standard input: {error}")))?;
    Ok(input)
}

fn split_nonempty_lines(input: &[u8]) -> Vec<&[u8]> {
    input
        .split(|byte| *byte == b'\n')
        .map(|line| line.strip_suffix(b"\r").unwrap_or(line))
        .filter(|line| !line.is_empty())
        .collect()
}

fn read_clipboard() -> Result<Vec<u8>> {
    #[cfg(windows)]
    let mut command = {
        let mut command = Command::new("powershell.exe");
        command.args(["-NoProfile", "-Command", "Get-Clipboard -Raw"]);
        command
    };
    #[cfg(not(windows))]
    let mut command = Command::new("xclip");
    #[cfg(not(windows))]
    command.args(["-selection", "clipboard", "-o"]);
    let output = command
        .stdout(Stdio::piped())
        .output()
        .map_err(|error| Error::Message(format!("cannot read clipboard: {error}")))?;
    if output.status.success() {
        Ok(output.stdout)
    } else {
        Err(Error::Message("clipboard command failed".to_owned()))
    }
}

fn print_results(results: &[ParseResult], format: Format, protocol: bool) -> Result<()> {
    if protocol {
        println!(
            "{}",
            serde_json::to_string_pretty(&BatchResult {
                schema_version: 1,
                results: results.to_vec(),
                rendered_json: results
                    .iter()
                    .map(output::render_json)
                    .collect::<Result<Vec<_>>>()?,
            })?
        );
        return Ok(());
    }
    if matches!(format, Format::Json) {
        if let [result] = results {
            println!("{}", output::render_json(result)?);
        } else {
            let values: Vec<_> = results.iter().map(output::clean_json_value).collect();
            println!("{}", serde_json::to_string_pretty(&values)?);
        }
        return Ok(());
    }
    for (index, result) in results.iter().enumerate() {
        if index > 0 {
            println!();
        }
        if results.len() > 1 {
            println!("MESSAGE {}", index + 1);
        }
        let output_format = match format {
            Format::Table => OutputFormat::Table,
            Format::Yaml => OutputFormat::Yaml,
            Format::Csv => OutputFormat::Csv,
            Format::Raw => OutputFormat::Raw,
            Format::Json => unreachable!(),
        };
        print!("{}", output::render(result, output_format)?);
        if !matches!(format, Format::Raw) {
            for diagnostic in result.warnings.iter().chain(&result.errors) {
                eprintln!("{}: {}", diagnostic.code, diagnostic.message);
            }
        }
    }
    Ok(())
}

fn list_command(project_name: Option<&str>, args: ListArgs) -> Result<u8> {
    let project = resolve_active_project(project_name)?;
    let mut rows = spec_rows(&project.specs_dir())?;
    if let Some(query) = args.query {
        let query = query.to_ascii_lowercase();
        rows.retain(|row| {
            serde_json::to_string(row)
                .unwrap_or_default()
                .to_ascii_lowercase()
                .contains(&query)
        });
    }
    if args.json {
        println!("{}", serde_json::to_string_pretty(&rows)?);
    } else {
        for row in rows {
            println!(
                "{:<10} {:>5} fields {:>7} bytes  {}  {}",
                row.service_code,
                row.fields,
                row.bytes,
                row.program.as_deref().unwrap_or("-"),
                row.endpoint.as_deref().unwrap_or("-")
            );
        }
    }
    Ok(0)
}

fn spec_rows(specs_dir: &Path) -> Result<Vec<SpecRow>> {
    let body = specs_dir.join("body");
    let mut rows = Vec::new();
    let entries = fs::read_dir(&body).map_err(|source| Error::Read {
        path: body.clone(),
        source,
    })?;
    for entry in entries {
        let entry = entry.map_err(|source| Error::Read {
            path: body.clone(),
            source,
        })?;
        if entry.path().extension().and_then(|value| value.to_str()) != Some("spec") {
            continue;
        }
        let spec = Spec::load(entry.path())?;
        let service_code = spec
            .metadata
            .get("service-code")
            .cloned()
            .or_else(|| {
                entry
                    .path()
                    .file_stem()
                    .map(|value| value.to_string_lossy().into_owned())
            })
            .unwrap_or_default();
        rows.push(SpecRow {
            service_code,
            spec_path: entry.path(),
            fields: spec
                .entries
                .iter()
                .map(|entry| match entry {
                    slix::spec::SpecEntry::Field(_) => 1,
                    slix::spec::SpecEntry::Repeat(group) => group.fields.len() * group.count,
                })
                .sum(),
            bytes: spec.byte_len(),
            endpoint: spec.metadata.get("endpoint").cloned(),
            category: spec.metadata.get("category").cloned(),
            section: spec.metadata.get("section").cloned(),
            program: spec.metadata.get("program").cloned(),
        });
    }
    rows.sort_by(|left, right| left.service_code.cmp(&right.service_code));
    Ok(rows)
}

fn project_command(command: Option<ProjectCommand>) -> Result<u8> {
    match command {
        None => {
            let project = resolve_active_project(None)?;
            println!("{}\t{}", project.config.name, project.root.display());
        }
        Some(ProjectCommand::List) => {
            let active = load_global_config()?.active_project;
            for project in list_projects()? {
                let marker = if active.as_deref() == Some(&project.config.name) {
                    "*"
                } else {
                    " "
                };
                println!(
                    "{marker} {}\t{}",
                    project.config.name,
                    project.root.display()
                );
            }
        }
        Some(ProjectCommand::Use { name }) => {
            set_active_project(&name)?;
            println!("Active project: {name}");
        }
        Some(ProjectCommand::Import {
            name,
            specs_directory,
        }) => {
            let project = import_project(&name, &specs_directory)?;
            println!(
                "Imported {} into {}",
                project.config.name,
                project.root.display()
            );
        }
    }
    Ok(0)
}

fn doctor(project_name: Option<&str>) -> Result<u8> {
    println!("Config: {}", config_root()?.display());
    match resolve_active_project(project_name) {
        Ok(project) => {
            println!("Project: {}", project.config.name);
            println!("Specs: {}", project.specs_dir().display());
            println!("Encoding: {}", project.config.encoding);
            Spec::load(project.specs_dir().join("header.spec"))?;
            println!("Header spec: OK");
        }
        Err(error) => println!("Project: ERROR ({error})"),
    }
    let nvim = Command::new("nvim").arg("--version").output();
    println!(
        "Neovim: {}",
        if nvim.is_ok_and(|value| value.status.success()) {
            "OK"
        } else {
            "not found"
        }
    );
    Ok(0)
}

fn stats(project_name: Option<&str>) -> Result<u8> {
    let project = resolve_active_project(project_name)?;
    let rows = spec_rows(&project.specs_dir())?;
    println!("{} specs", rows.len());
    println!(
        "{} fields",
        rows.iter().map(|row| row.fields).sum::<usize>()
    );
    println!(
        "{} endpoints",
        rows.iter().filter(|row| row.endpoint.is_some()).count()
    );
    println!(
        "{} programs",
        rows.iter().filter(|row| row.program.is_some()).count()
    );
    Ok(0)
}

fn find_spec(specs_dir: &Path, code: &str) -> Option<PathBuf> {
    let code = code.trim().trim_start_matches('#').to_ascii_uppercase();
    [format!("{code}.spec"), format!("#{code}.spec")]
        .into_iter()
        .map(|name| specs_dir.join("body").join(name))
        .find(|path| path.is_file())
}
