//! Writes `api-docs/`, the app-author API reference, from the rustdoc
//! JSON of the `choscorui` facade and the workspace crates that it
//! re-exports.
//!
//! ```sh
//! cargo run -p choscorui-api-docs --locked            # write api-docs/
//! cargo run -p choscorui-api-docs --locked -- --check # compare only
//! ```
//!
//! The JSON comes from the nightly toolchain in `nightly.txt`. Exit
//! status: 0 on success, 1 when the pages are stale or a scope or content
//! rule fails, 2 when the toolchain, rustdoc, or a file operation fails.

mod doc;
mod json;
mod model;
mod render;
mod scope;
mod sig;
#[cfg(test)]
mod tests;

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

use model::{Krate, World};

/// The target whose rustdoc JSON the pages show.
const TARGET: &str = "aarch64-apple-darwin";
/// The facade package.
const FACADE: &str = "choscorui";
/// Crates whose names start with this are workspace crates.
const WORKSPACE_PREFIX: &str = "choscorui_";
/// The output directory, relative to the workspace root.
const OUT_DIR: &str = "api-docs";

/// A failure with its exit status and message.
struct Failure {
    status: u8,
    message: String,
}

impl Failure {
    fn environment(message: impl Into<String>) -> Failure {
        Failure {
            status: 2,
            message: message.into(),
        }
    }

    fn rules(lines: &[String]) -> Failure {
        Failure {
            status: 1,
            message: lines.join("\n"),
        }
    }
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let check = match args.as_slice() {
        [] => false,
        [flag] if flag == "--check" => true,
        _ => {
            eprintln!("usage: choscorui-api-docs [--check]");
            return ExitCode::from(2);
        }
    };
    match run(check) {
        Ok(message) => {
            println!("{message}");
            ExitCode::SUCCESS
        }
        Err(failure) => {
            eprintln!("api-docs: {}", failure.message);
            ExitCode::from(failure.status)
        }
    }
}

fn workspace_root() -> PathBuf {
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    manifest
        .parent()
        .and_then(Path::parent)
        .unwrap_or(manifest)
        .to_path_buf()
}

fn run(check: bool) -> Result<String, Failure> {
    let root = workspace_root();
    let tool = root.join("tools/api-docs");
    let pin = read_pin(&tool.join("nightly.txt"))?;
    let topics = std::fs::read_to_string(tool.join("topics.txt")).map_err(|error| {
        Failure::environment(format!("cannot read tools/api-docs/topics.txt: {error}"))
    })?;
    let world = load_world(&root, &pin)?;
    let pages = generate(&world, &topics)?;
    let out = root.join(OUT_DIR);
    if check {
        compare(&out, &pages)
    } else {
        write(&out, &pages)
    }
}

/// Renders the pages from the JSON and the topics config.
fn generate(world: &World, topics: &str) -> Result<BTreeMap<String, String>, Failure> {
    let lines = scope::parse_topics(topics).map_err(|errors| Failure::rules(&errors))?;
    let scope = scope::collect(world, &lines).map_err(|errors| Failure::rules(&errors))?;
    render::render(world, &scope).map_err(|errors| Failure::rules(&errors))
}

fn read_pin(path: &Path) -> Result<String, Failure> {
    let text = std::fs::read_to_string(path).map_err(|error| {
        Failure::environment(format!("cannot read {}: {error}", path.display()))
    })?;
    text.lines()
        .map(str::trim)
        .find(|line| !line.is_empty() && !line.starts_with('#'))
        .map(str::to_owned)
        .ok_or_else(|| Failure::environment("tools/api-docs/nightly.txt names no toolchain"))
}

/// A `cargo +<pin>` command without the settings of the calling cargo,
/// which belong to a different toolchain.
fn cargo(pin: &str, root: &Path) -> Command {
    let mut command = Command::new("cargo");
    command
        .arg(format!("+{pin}"))
        .current_dir(root)
        .env("RUSTUP_AUTO_INSTALL", "0");
    for name in [
        "RUSTUP_TOOLCHAIN",
        "RUSTC",
        "RUSTDOC",
        "RUSTC_WRAPPER",
        "RUSTFLAGS",
        "RUSTDOCFLAGS",
        "CARGO_ENCODED_RUSTFLAGS",
        "CARGO_ENCODED_RUSTDOCFLAGS",
        "CARGO_TARGET_DIR",
        "CARGO_BUILD_TARGET",
    ] {
        command.env_remove(name);
    }
    command
}

fn check_toolchain(pin: &str, root: &Path) -> Result<(), Failure> {
    let output = cargo(pin, root).arg("--version").output();
    let installed = matches!(&output, Ok(output) if output.status.success());
    if installed {
        return Ok(());
    }
    let detail = match output {
        Ok(output) => String::from_utf8_lossy(&output.stderr).trim().to_owned(),
        Err(error) => error.to_string(),
    };
    Err(Failure::environment(format!(
        "the pinned toolchain {pin} is not usable ({detail}).\n\
         Install it with: rustup toolchain install {pin} --profile minimal --target {TARGET}"
    )))
}

/// Runs rustdoc for each crate that the facade re-exports and loads the
/// JSON files.
fn load_world(root: &Path, pin: &str) -> Result<World, Failure> {
    check_toolchain(pin, root)?;
    let target_dir = root.join("target/api-docs");
    let facade = rustdoc(root, pin, FACADE, &target_dir)?;
    let mut names: Vec<String> = facade
        .external_names()
        .filter(|name| name.starts_with(WORKSPACE_PREFIX))
        .map(str::to_owned)
        .collect();
    names.sort();
    names.dedup();
    let mut crates = vec![facade];
    for name in names {
        crates.push(rustdoc(root, pin, &name.replace('_', "-"), &target_dir)?);
    }
    Ok(World::new(crates))
}

fn rustdoc(root: &Path, pin: &str, package: &str, target_dir: &Path) -> Result<Krate, Failure> {
    let mut command = cargo(pin, root);
    command.args([
        "rustdoc",
        "--quiet",
        "--locked",
        "--lib",
        "-p",
        package,
        "--target",
        TARGET,
        "--target-dir",
    ]);
    command.arg(target_dir);
    command.args(["--", "-Z", "unstable-options", "--output-format", "json"]);
    let shown = format!(
        "cargo +{pin} rustdoc --quiet --locked --lib -p {package} --target {TARGET} --target-dir {} \
         -- -Z unstable-options --output-format json",
        target_dir.display()
    );
    let output = command
        .output()
        .map_err(|error| Failure::environment(format!("cannot run `{shown}`: {error}")))?;
    if !output.status.success() {
        return Err(Failure::environment(format!(
            "`{shown}` failed:\n{}",
            String::from_utf8_lossy(&output.stderr).trim_end()
        )));
    }
    let path = target_dir
        .join(TARGET)
        .join("doc")
        .join(format!("{}.json", package.replace('-', "_")));
    let text = std::fs::read_to_string(&path).map_err(|error| {
        Failure::environment(format!("cannot read {}: {error}", path.display()))
    })?;
    let json = json::parse(&text)
        .map_err(|error| Failure::environment(format!("{}: {error}", path.display())))?;
    Krate::from_json(json)
        .map_err(|error| Failure::environment(format!("{}: {error}", path.display())))
}

/// The Markdown files now in the output directory, by name.
fn existing(out: &Path) -> Result<BTreeMap<String, PathBuf>, Failure> {
    let mut files = BTreeMap::new();
    let entries = match std::fs::read_dir(out) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(files),
        Err(error) => {
            return Err(Failure::environment(format!(
                "cannot read {}: {error}",
                out.display()
            )));
        }
    };
    for entry in entries {
        let entry = entry.map_err(|error| {
            Failure::environment(format!("cannot read {}: {error}", out.display()))
        })?;
        files.insert(
            entry.file_name().to_string_lossy().into_owned(),
            entry.path(),
        );
    }
    Ok(files)
}

fn compare(out: &Path, pages: &BTreeMap<String, String>) -> Result<String, Failure> {
    let present = existing(out)?;
    let mut problems = Vec::new();
    for (name, text) in pages {
        match present.get(name) {
            None => problems.push(format!("missing: {OUT_DIR}/{name}")),
            Some(path) => {
                if std::fs::read_to_string(path).ok().as_deref() != Some(text.as_str()) {
                    problems.push(format!("stale: {OUT_DIR}/{name}"));
                }
            }
        }
    }
    for name in present.keys() {
        if !pages.contains_key(name) {
            problems.push(format!("extra: {OUT_DIR}/{name}"));
        }
    }
    if problems.is_empty() {
        return Ok(format!("{OUT_DIR}/ is current ({} files)", pages.len()));
    }
    problems.push(format!("regenerate with: {}", render::REGENERATE));
    Err(Failure::rules(&problems))
}

fn write(out: &Path, pages: &BTreeMap<String, String>) -> Result<String, Failure> {
    let io = |error: std::io::Error| {
        Failure::environment(format!("cannot write {}: {error}", out.display()))
    };
    std::fs::create_dir_all(out).map_err(io)?;
    for (name, path) in existing(out)? {
        if !pages.contains_key(&name) {
            std::fs::remove_file(&path).map_err(io)?;
        }
    }
    for (name, text) in pages {
        let path = out.join(name);
        if std::fs::read_to_string(&path).ok().as_deref() != Some(text.as_str()) {
            std::fs::write(&path, text).map_err(io)?;
        }
    }
    Ok(format!("wrote {} files to {OUT_DIR}/", pages.len()))
}
