mod process;

use std::{
    collections::BTreeMap,
    env,
    ffi::OsString,
    fs,
    net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr, TcpListener, TcpStream, ToSocketAddrs},
    path::{Path, PathBuf},
    process::{Command, ExitCode},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread,
    time::{Duration, Instant, SystemTime},
};

use anyhow::{Context, Result, bail, ensure};
use clap::{CommandFactory, Parser, Subcommand};
use process::{Build, Process};

#[derive(Debug, Parser)]
#[command(
    name = "xtask",
    bin_name = "cargo xtask",
    about = "Develop, build, and maintain the llm-bridge workspace",
    after_help = "Install dependencies explicitly first: pnpm --dir frontend install --frozen-lockfile"
)]
struct Cli {
    #[command(subcommand)]
    task: Option<Task>,
}

#[derive(Debug, Subcommand)]
enum Task {
    /// Run Vite and rebuild the backend when Rust inputs change
    #[command(
        after_help = "LLM_BRIDGE_PORT=3000, LLM_BRIDGE_UI_PORT=5173, LLM_BRIDGE_HOST=127.0.0.1.\nLLM_BRIDGE_BASE_URL defaults to the Vite URL; an explicit value is preserved.\nFrontend edits use Vite HMR. Ctrl-C stops all children."
    )]
    Dev {
        /// Additional Cargo features, comma- or space-separated; may be repeated
        #[arg(long, value_name = "LIST")]
        features: Vec<String>,
    },
    /// Build the frontend and an embedded backend (release by default)
    Build {
        /// Select Cargo's dev profile instead of release
        #[arg(long, overrides_with = "debug")]
        debug: bool,
        /// Additional Cargo features, comma- or space-separated; may be repeated
        #[arg(long, value_name = "LIST")]
        features: Vec<String>,
        /// Compile the backend for this target triple
        #[arg(long, value_name = "TRIPLE", value_parser = clap::builder::NonEmptyStringValueParser::new())]
        target: Option<String>,
    },
    /// Generate Rust type bindings and the TypeScript API client
    Bindings {
        /// Check for drift without overwriting committed bindings
        #[arg(long, overrides_with = "check")]
        check: bool,
    },
}

fn normalize_features(lists: Vec<String>) -> Result<Vec<String>> {
    let mut features = Vec::new();
    for list in lists {
        ensure!(
            !list.trim().is_empty(),
            "--features requires a feature list"
        );
        features.extend(
            list.split([',', ' '])
                .filter(|feature| !feature.is_empty())
                .map(str::to_owned),
        );
    }
    features.sort();
    features.dedup();
    Ok(features)
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("[xtask] {error:#}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<()> {
    let Some(task) = Cli::parse().task else {
        Cli::command().print_help()?;
        println!();
        return Ok(());
    };
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .context("xtask must be inside the workspace")?;
    match task {
        Task::Dev { features } => {
            let features = normalize_features(features)?;
            ensure!(
                !features
                    .iter()
                    .any(|f| matches!(f.rsplit('/').next(), Some("embed-frontend"))),
                "dev serves assets through Vite; embed-frontend is only available with build"
            );
            dev(root, &features)?;
        }
        Task::Build {
            debug,
            features,
            target,
        } => {
            let mut features = normalize_features(features)?;
            require_frontend(root)?;
            run_command(root.join("frontend").as_path(), pnpm(), &["run", "build"])?;
            features.push("embed-frontend".into());
            features.sort();
            features.dedup();
            let mut args = cargo_build_args(&features);
            if !debug {
                args.push("--release".into());
            }
            if let Some(target) = target {
                args.extend(["--target".into(), target]);
            }
            run_command(root, cargo(), &args)?;
        }
        Task::Bindings { check: true } => {
            run_command(root, "python3", &["scripts/check-bindings.py"])?;
        }
        Task::Bindings { check: false } => {
            run_command(
                root,
                cargo(),
                &[
                    "test",
                    "-p",
                    "llm-bridge",
                    "--lib",
                    "--locked",
                    "export_bindings",
                ],
            )?;
            run_command(
                root,
                cargo(),
                &[
                    "test",
                    "-p",
                    "llm-bridge",
                    "--locked",
                    "--test",
                    "generate_ts_client",
                    "generate_ts_client",
                    "--",
                    "--ignored",
                    "--exact",
                ],
            )?;
        }
    }
    Ok(())
}

fn cargo() -> OsString {
    env::var_os("CARGO").unwrap_or_else(|| "cargo".into())
}

fn pnpm() -> &'static str {
    if cfg!(windows) { "pnpm.cmd" } else { "pnpm" }
}

fn run_command(
    root: &Path,
    program: impl AsRef<std::ffi::OsStr>,
    args: &[impl AsRef<std::ffi::OsStr>],
) -> Result<()> {
    eprintln!(
        "[xtask] running {:?} in {}",
        program.as_ref(),
        root.display()
    );
    duct::cmd(program.as_ref(), args.iter().map(AsRef::as_ref))
        .dir(root)
        .run()?;
    Ok(())
}

fn require_frontend(root: &Path) -> Result<()> {
    ensure!(
        root.join("frontend/node_modules/vite/package.json")
            .is_file(),
        "frontend dependencies are missing; run `pnpm --dir frontend install --frozen-lockfile`"
    );
    Ok(())
}

fn cargo_build_args(features: &[String]) -> Vec<String> {
    let mut args = [
        "build",
        "-p",
        "llm-bridge",
        "--bin",
        "llm-bridge",
        "--no-default-features",
        "--locked",
    ]
    .map(str::to_owned)
    .to_vec();
    if !features.is_empty() {
        args.extend(["--features".into(), features.join(",")]);
    }
    args
}

fn port(name: &str, default: u16) -> Result<u16> {
    match env::var(name) {
        Ok(value) => {
            ensure!(
                !value.is_empty() && value.bytes().all(|b| b.is_ascii_digit()),
                "{name} must be an integer between 1 and 65535"
            );
            let port: u16 = value
                .parse()
                .with_context(|| format!("{name} must be between 1 and 65535"))?;
            ensure!(port != 0, "{name} must not be zero");
            Ok(port)
        }
        Err(env::VarError::NotPresent) => Ok(default),
        Err(error) => Err(error).with_context(|| format!("invalid {name}")),
    }
}

fn ensure_free(address: SocketAddr) -> Result<()> {
    TcpListener::bind(address).with_context(|| {
        format!("{address} is unavailable; stop its owner or select another port")
    })?;
    Ok(())
}

fn listening(address: SocketAddr) -> bool {
    TcpStream::connect_timeout(&address, Duration::from_millis(100)).is_ok()
}

fn dev(root: &Path, features: &[String]) -> Result<()> {
    require_frontend(root)?;
    let backend_port = port("LLM_BRIDGE_PORT", 3000)?;
    let ui = SocketAddr::from((Ipv4Addr::LOCALHOST, port("LLM_BRIDGE_UI_PORT", 5173)?));
    let host = env::var("LLM_BRIDGE_HOST").unwrap_or_else(|_| "127.0.0.1".into());
    let bind: SocketAddr = format!("{host}:{backend_port}")
        .to_socket_addrs()
        .context("cannot resolve LLM_BRIDGE_HOST")?
        .next()
        .context("LLM_BRIDGE_HOST resolved to no addresses")?;
    let mut backend_address = bind;
    if bind.ip().is_unspecified() {
        backend_address.set_ip(match bind.ip() {
            IpAddr::V4(_) => Ipv4Addr::LOCALHOST.into(),
            IpAddr::V6(_) => Ipv6Addr::LOCALHOST.into(),
        });
    }
    ensure!(
        backend_address != ui,
        "frontend and backend must use different addresses"
    );
    ensure_free(bind)?;
    ensure_free(ui)?;
    let public_url = format!("http://{ui}");
    let base_url = env::var("LLM_BRIDGE_BASE_URL").unwrap_or_else(|_| public_url.clone());
    let stop = Arc::new(AtomicBool::new(false));
    let signal_stop = Arc::clone(&stop);
    ctrlc::set_handler(move || signal_stop.store(true, Ordering::Relaxed))
        .context("cannot install development shutdown handler")?;

    let mut vite_command = Command::new(pnpm());
    vite_command
        .args(["run", "dev"])
        .current_dir(root.join("frontend"))
        .env("LLM_BRIDGE_UI_PORT", ui.port().to_string())
        .env(
            "LLM_BRIDGE_DEV_BACKEND_URL",
            format!("http://{backend_address}"),
        );
    let mut vite = Process::spawn("Vite", vite_command)?;
    let vite_deadline = Instant::now() + Duration::from_secs(60);
    while !listening(ui) {
        if stop.load(Ordering::Relaxed) {
            return Ok(());
        }
        if let Some(status) = vite.poll()? {
            bail!("Vite exited before becoming ready: {status}");
        }
        ensure!(
            Instant::now() < vite_deadline,
            "Vite did not listen on {ui} within 60 seconds"
        );
        thread::sleep(Duration::from_millis(100));
    }

    let mut snapshot = inputs(root)?;
    let mut last_scan = Instant::now();
    let mut dirty = Some(Instant::now() - Duration::from_secs(1));
    let mut build: Option<Build> = None;
    let mut backend: Option<Process> = None;
    let mut backend_deadline = None;
    eprintln!("[xtask] Vite: {public_url}; public base URL: {base_url}");
    eprintln!("[xtask] watching Rust inputs; Ctrl-C stops all processes");
    while !stop.load(Ordering::Relaxed) {
        if let Some(status) = vite.poll()? {
            bail!("Vite exited unexpectedly: {status}");
        }
        if let Some(server) = backend.as_mut() {
            if let Some(status) = server.poll()? {
                bail!("backend exited unexpectedly: {status}");
            }
            if let Some(deadline) = backend_deadline {
                if listening(backend_address) {
                    backend_deadline = None;
                    eprintln!("[xtask] ready: {public_url} -> http://{backend_address}");
                } else {
                    ensure!(
                        Instant::now() < deadline,
                        "backend did not listen on {backend_address} within 60 seconds"
                    );
                }
            }
        }
        if last_scan.elapsed() >= Duration::from_millis(500) {
            let current = inputs(root)?;
            if current != snapshot {
                snapshot = current;
                dirty = Some(Instant::now());
            }
            last_scan = Instant::now();
        }
        if build.is_none()
            && dirty.is_some_and(|changed| changed.elapsed() >= Duration::from_millis(300))
        {
            if let Some(mut server) = backend.take() {
                server.stop()?;
            }
            backend_deadline = None;
            ensure_free(bind)?;
            let mut command = Command::new(cargo());
            command.args(cargo_build_args(features)).current_dir(root);
            build = Some(Build::start(command)?);
            dirty = None;
        }
        if let Some(compiling) = build.as_mut()
            && let Some(status) = compiling.process.poll()?
        {
            let executable = compiling.executable()?;
            drop(build.take());
            if !status.success() {
                eprintln!(
                    "[xtask] Rust build failed ({status}); Vite remains running, waiting for Rust changes"
                );
            } else if dirty.is_none() {
                let executable = executable
                    .context("Cargo succeeded without reporting the llm-bridge executable")?;
                let mut command = Command::new(executable);
                command
                    .current_dir(root)
                    .env("LLM_BRIDGE_HOST", &host)
                    .env("LLM_BRIDGE_PORT", backend_port.to_string())
                    .env("LLM_BRIDGE_BASE_URL", &base_url);
                backend = Some(Process::spawn("backend", command)?);
                backend_deadline = Some(Instant::now() + Duration::from_secs(60));
            }
        }
        thread::sleep(Duration::from_millis(100));
    }
    eprintln!("[xtask] stopping development processes");
    Ok(())
}

type Snapshot = BTreeMap<PathBuf, (SystemTime, u64)>;

fn inputs(root: &Path) -> Result<Snapshot> {
    fn collect(path: &Path, result: &mut Snapshot) -> Result<()> {
        let entries = match fs::read_dir(path) {
            Ok(entries) => entries,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
            Err(error) => return Err(error.into()),
        };
        for entry in entries {
            let entry = entry?;
            let kind = entry.file_type()?;
            if kind.is_dir() {
                collect(&entry.path(), result)?;
            } else if kind.is_file() || kind.is_symlink() {
                let metadata = match fs::metadata(entry.path()) {
                    Ok(metadata) => metadata,
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
                    Err(error) => return Err(error.into()),
                };
                if metadata.is_file() {
                    result.insert(entry.path(), (metadata.modified()?, metadata.len()));
                }
            }
        }
        Ok(())
    }
    let mut result = BTreeMap::new();
    collect(&root.join("src"), &mut result).context("cannot scan Rust sources")?;
    for file in [
        "Cargo.toml",
        "Cargo.lock",
        "build.rs",
        ".cargo/config.toml",
        "rust-toolchain.toml",
    ] {
        let path = root.join(file);
        match fs::metadata(&path) {
            Ok(metadata) => {
                result.insert(path, (metadata.modified()?, metadata.len()));
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error).with_context(|| format!("cannot watch {file}")),
        }
    }
    Ok(result)
}
