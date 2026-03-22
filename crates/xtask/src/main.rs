use std::fs;
use std::net::{SocketAddr, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use clap::{Parser, Subcommand};
use miette::{Context, IntoDiagnostic};
use serde::{Deserialize, Serialize};
use tracing_subscriber::EnvFilter;

#[derive(Debug, Parser)]
#[command(name = "xtask")]
#[command(about = "Workspace automation tasks for forge", long_about = None)]
pub struct Xtask {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Debug, Subcommand)]
pub enum Cmd {
    /// Start a forged server suitable for e2e tests and print JSON with connection details.
    ///
    /// Requires a running PostgreSQL instance. Configure via TEST_DATABASE_URL
    /// (default: postgresql://forged:forged@localhost/forged_test).
    SetupTestEnv {
        /// Optional directory to create under target/ for this env (default: random)
        #[arg(long)]
        name: Option<String>,
        /// Timeout to wait for server to accept connections (seconds)
        #[arg(long, default_value_t = 15)]
        timeout: u64,
        /// PostgreSQL connection URL for the test database
        #[arg(
            long,
            default_value = "postgresql://forged:forged@localhost/forged_test"
        )]
        database_url: String,
    },
    /// Build an illumos sysroot on Linux by downloading packages from an OmniOS IPS repo (no pkg(5))
    Sysroot {
        /// IPS repository URL (e.g., https://pkg.omnios.org/r151052/core)
        #[arg(long, default_value = "https://pkg.omnios.org/r151052/core")]
        repo: String,
        /// Publisher name (e.g., omnios)
        #[arg(long, default_value = "omnios")]
        publisher: String,
        /// Space/comma-separated list of package FMRIs or stems to include (e.g., "library/security/openssl library/libarchive developer/build/pkg-config")
        #[arg(long, value_delimiter = ' ', num_args = 1.., required = true)]
        packages: Vec<String>,
        /// Output directory to place the sysroot tarball and extracted dir
        #[arg(long, default_value = "sysroots")]
        out_dir: PathBuf,
        /// Optional name override for the output base name
        #[arg(long)]
        name: Option<String>,
    },
}

#[derive(Debug, Serialize, Deserialize)]
pub struct TestEnvInfo {
    pub addr: String,
    pub database_url: String,
    pub pid: u32,
    pub dir: String,
}

fn main() -> miette::Result<()> {
    let env_filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    tracing_subscriber::fmt().with_env_filter(env_filter).init();

    let xt = Xtask::parse();
    match xt.cmd {
        Cmd::SetupTestEnv {
            name,
            timeout,
            database_url,
        } => {
            let info = setup_test_env(name.as_deref(), Duration::from_secs(timeout), &database_url)
                .wrap_err("setup test env failed")?;
            println!("{}", serde_json::to_string_pretty(&info).into_diagnostic()?);
            Ok(())
        }
        Cmd::Sysroot {
            repo,
            publisher,
            packages,
            out_dir,
            name,
        } => {
            build_sysroot(&repo, &publisher, &packages, &out_dir, name.as_deref())?;
            Ok(())
        }
    }
}

fn setup_test_env(
    name: Option<&str>,
    timeout: Duration,
    database_url: &str,
) -> miette::Result<TestEnvInfo> {
    // Directories under target
    let target_dir = PathBuf::from("target");
    fs::create_dir_all(&target_dir).into_diagnostic()?;
    let dir_name = name
        .map(|s| s.to_string())
        .unwrap_or_else(|| format!("forged-test-{}", uuid::Uuid::new_v4()));
    let env_dir = target_dir.join(dir_name);
    fs::create_dir_all(&env_dir).into_diagnostic()?;

    // Pick a free port
    let port = portpicker::pick_unused_port().unwrap_or(50051);
    let addr = format!("127.0.0.1:{}", port);

    // Spawn `cargo run -p forged`
    let mut cmd = Command::new("cargo");
    cmd.arg("run")
        .arg("-p")
        .arg("forged")
        .arg("--quiet")
        .env("FORGED__SERVER__LISTEN_ADDR", &addr)
        .env("FORGED__POSTGRES__URL", database_url)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .current_dir(project_root());

    let child = cmd.spawn().into_diagnostic().wrap_err("spawn forged")?;

    wait_for_server(&addr, timeout)?;

    let info = TestEnvInfo {
        addr: addr.clone(),
        database_url: database_url.to_string(),
        pid: child.id(),
        dir: env_dir.display().to_string(),
    };

    // Write a small metadata file for convenience
    let meta_path = env_dir.join("env.json");
    fs::write(
        &meta_path,
        serde_json::to_vec_pretty(&info).into_diagnostic()?,
    )
    .into_diagnostic()
    .wrap_err_with(|| format!("write {}", meta_path.display()))?;

    // Detach child; The caller is responsible for killing the process after tests.
    std::mem::forget(child);

    Ok(info)
}

fn project_root() -> PathBuf {
    // Assume xtask runs in workspace root; use CARGO_MANIFEST_DIR to derive root/crates/xtask -> root
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    manifest_dir
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .to_path_buf()
}

fn wait_for_server(addr: &str, timeout: Duration) -> miette::Result<()> {
    let start = Instant::now();
    loop {
        if start.elapsed() > timeout {
            return Err(miette::miette!(
                "server did not start listening at {} within {:?}",
                addr,
                timeout
            ));
        }
        match addr.parse::<SocketAddr>() {
            Ok(sa) => {
                if TcpStream::connect_timeout(&sa, Duration::from_millis(200)).is_ok() {
                    return Ok(());
                }
            }
            Err(_) => return Err(miette::miette!("invalid addr: {}", addr)),
        }
        thread::sleep(Duration::from_millis(200));
    }
}

// Default constants for sysroot builder (can be extended easily)
const DEFAULT_REPO: &str = "https://pkg.omnios.org/r151052/core";
const DEFAULT_PUBLISHER: &str = "omnios";
const DEFAULT_PACKAGES: &[&str] = &[
    "library/security/openssl",
    "library/libarchive",
    "developer/pkg-config",
];

fn build_sysroot(
    repo: &str,
    publisher: &str,
    packages: &[String],
    out_dir: &Path,
    name: Option<&str>,
) -> miette::Result<()> {
    use miette::WrapErr as _;

    // Normalize inputs and defaults
    let repo = if repo.is_empty() { DEFAULT_REPO } else { repo };
    let publisher = if publisher.is_empty() {
        DEFAULT_PUBLISHER
    } else {
        publisher
    };
    let pkg_list: Vec<String> = if packages.is_empty() {
        DEFAULT_PACKAGES.iter().map(|s| s.to_string()).collect()
    } else {
        packages.to_vec()
    };

    // Prepare output paths
    fs::create_dir_all(out_dir).into_diagnostic()?;

    let stamp = {
        use std::time::{SystemTime, UNIX_EPOCH};
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_else(|_| std::time::Duration::from_secs(0))
            .as_secs();
        format!("{}", now)
    };
    let base_name = name
        .map(|s| s.to_string())
        .unwrap_or_else(|| format!("illumos-sysroot-{}-{}", publisher, stamp));
    let tar_path = out_dir.join(format!("{}.tar.gz", base_name));
    let extract_dir = out_dir.join(&base_name);
    let sysroot_dir = extract_dir.join("sysroot");

    // Ensure clean sysroot directory
    fs::create_dir_all(&sysroot_dir)
        .into_diagnostic()
        .wrap_err_with(|| format!("create sysroot dir {}", sysroot_dir.display()))?;

    // 1) Create an IPS image rooted at the sysroot directory and configure publisher
    {
        use libips::image::{Image, ImageType};

        // Create the image metadata/layout (pkg6.image.json, etc.)
        let mut img = Image::create_image(&sysroot_dir, ImageType::Full)
            .into_diagnostic()
            .wrap_err_with(|| format!("create image at {}", sysroot_dir.display()))?;

        // Add the publisher with the given origin URL
        img.add_publisher(publisher, repo, vec![], true)
            .into_diagnostic()
            .wrap_err_with(|| format!("add publisher '{}' with origin {}", publisher, repo))?;

        // If using OmniOS 'core', also add the matching 'extra' publisher (extra.omnios).
        let mut publishers_for_refresh = vec![publisher.to_string()];
        if publisher.eq_ignore_ascii_case("omnios") {
            if let Some(prefix) = repo.strip_suffix("/core") {
                let extra_url = format!("{}/extra", prefix);
                let extra_pub = "extra.omnios".to_string();
                match img.add_publisher(&extra_pub, &extra_url, vec![], true) {
                    Ok(()) => {
                        tracing::info!(publisher = %extra_pub, origin = %extra_url, "added extra publisher for OmniOS extra");
                        publishers_for_refresh.push(extra_pub);
                    }
                    Err(e) => {
                        tracing::warn!(origin = %extra_url, error = %e, "failed to add extra.omnios publisher; continuing with existing publishers");
                    }
                }
            }
        }

        // Refresh catalogs (full) so the solver has data available
        img.refresh_catalogs(&publishers_for_refresh, true)
            .into_diagnostic()
            .wrap_err("refresh catalogs")?;
    }

    // Re-open the image handle for subsequent operations
    let image = libips::image::Image::load(&sysroot_dir)
        .into_diagnostic()
        .wrap_err_with(|| format!("load image at {}", sysroot_dir.display()))?;

    // 2) Build install constraints (stems, latest)
    // Build list of preferred publishers; include extra.omnios automatically for OmniOS
    let mut preferred_pubs = vec![publisher.to_string()];
    if publisher.eq_ignore_ascii_case("omnios") {
        preferred_pubs.push("extra.omnios".to_string());
    }

    let build_constraints = |stems: &[String]| -> Vec<libips::solver::Constraint> {
        stems
            .iter()
            .map(|stem| libips::solver::Constraint {
                stem: stem.clone(),
                version_req: None,
                preferred_publishers: preferred_pubs.clone(),
                branch: None,
            })
            .collect()
    };

    let stems = pkg_list.clone();

    // 4) Resolve and apply install plan
    let plan = match libips::solver::resolve_install(&image, &build_constraints(&stems)) {
        Ok(p) => p,
        Err(e) => {
            let msg = format!("{}", e);
            // Fallbacks for libarchive on OmniOS:
            // 1) archiver/libarchive (some distros)
            // 2) ooce/library/libarchive (OmniOS extra)
            if stems.iter().any(|s| s == "library/libarchive") {
                let mut last_err: Option<miette::Report> = None;
                let mut fallback_plan: Option<libips::solver::InstallPlan> = None;
                for alt_stem in ["archiver/libarchive", "ooce/library/libarchive"].iter() {
                    let mut alt = stems.clone();
                    for s in &mut alt {
                        if s == "library/libarchive" {
                            *s = alt_stem.to_string();
                        }
                    }
                    tracing::info!(
                        "retrying resolution with {} instead of library/libarchive",
                        alt_stem
                    );
                    match libips::solver::resolve_install(&image, &build_constraints(&alt)) {
                        Ok(p) => {
                            fallback_plan = Some(p);
                            break;
                        }
                        Err(e2) => {
                            last_err =
                                Some(miette::miette!("fallback with {} failed: {}", alt_stem, e2));
                        }
                    }
                }
                // If fallback succeeded, use that plan; else return the last error
                if let Some(p) = fallback_plan {
                    p
                } else {
                    return Err(last_err.unwrap_or_else(|| {
                        miette::miette!("resolve install plan for sysroot failed: {}", msg)
                    }));
                }
            } else {
                return Err(miette::miette!(
                    "resolve install plan for sysroot failed: {}",
                    msg
                ));
            }
        }
    };

    let ap = libips::image::action_plan::ActionPlan::from_install_plan(&plan);

    // Progress callback to observe apply phases
    use std::sync::Arc;
    let progress_cb: libips::actions::executors::ProgressCallback = Arc::new(|evt| {
        match evt {
            libips::actions::executors::ProgressEvent::StartingPhase { phase, total } => {
                tracing::info!(%phase, total, "apply: starting phase");
            }
            libips::actions::executors::ProgressEvent::Progress {
                phase,
                current,
                total,
            } => {
                // keep noise moderate
                if total > 0 {
                    let pct = (current as f64 / total as f64) * 100.0;
                    tracing::debug!(%phase, current, total, pct = format!("{pct:.1}"), "apply: progress");
                } else {
                    tracing::debug!(%phase, current, total, "apply: progress");
                }
            }
            libips::actions::executors::ProgressEvent::FinishedPhase { phase, total } => {
                tracing::info!(%phase, total, "apply: finished phase");
            }
        }
    });

    let apply_opts = libips::actions::executors::ApplyOptions {
        dry_run: false,
        progress: Some(progress_cb),
        progress_interval: 25,
    };

    tracing::info!("applying action plan (dry-run: false)");
    ap.apply(image.path(), &apply_opts)
        .into_diagnostic()
        .wrap_err("apply install plan into sysroot")?;

    // 5) Create tar.gz of the populated sysroot
    fs::create_dir_all(&extract_dir).into_diagnostic()?;
    let tar_file = fs::File::create(&tar_path)
        .into_diagnostic()
        .wrap_err_with(|| format!("create {}", tar_path.display()))?;
    let enc = flate2::write::GzEncoder::new(tar_file, flate2::Compression::default());
    let mut tar_builder = tar::Builder::new(enc);

    // Recursively append sysroot but skip var/pkg (IPS image databases)
    fn append_filtered(
        builder: &mut tar::Builder<flate2::write::GzEncoder<std::fs::File>>,
        sysroot: &Path,
        dir: &Path,
    ) -> miette::Result<()> {
        for entry in fs::read_dir(dir).into_diagnostic()? {
            let entry = entry.into_diagnostic()?;
            let path = entry.path();
            let rel = path.strip_prefix(sysroot).unwrap();
            // Skip var/pkg
            if rel.components().take(2).collect::<Vec<_>>()
                == [
                    std::path::Component::Normal("var".as_ref()),
                    std::path::Component::Normal("pkg".as_ref()),
                ]
            {
                continue;
            }
            let dst = Path::new("sysroot").join(rel);
            if path.is_dir() {
                builder.append_dir(&dst, &path).into_diagnostic()?;
                append_filtered(builder, sysroot, &path)?;
            } else if path.is_file() {
                builder
                    .append_path_with_name(&path, &dst)
                    .into_diagnostic()?;
            }
        }
        Ok(())
    }

    append_filtered(&mut tar_builder, &sysroot_dir, &sysroot_dir)?;
    tar_builder.into_inner().into_diagnostic()?; // flush gz

    println!("[xtask sysroot] Repo: {}", repo);
    println!("[xtask sysroot] Publisher: {}", publisher);
    println!("[xtask sysroot] Packages: {}", pkg_list.join(", "));
    println!("[xtask sysroot] Tarball: {}", tar_path.display());
    println!("[xtask sysroot] Sysroot dir: {}", sysroot_dir.display());

    // Print environment hints for cross
    println!("[xtask sysroot] Example env for cross: ");
    let s = sysroot_dir.display();
    println!(
        "  SYSROOT=\"{s}\" \\\n  PKG_CONFIG_ALLOW_CROSS=1 \\\n  PKG_CONFIG_SYSROOT_DIR=\"{s}\" \\\n  PKG_CONFIG_LIBDIR=\"{s}/usr/lib/amd64/pkgconfig:{s}/usr/lib/64/pkgconfig:{s}/usr/lib/pkgconfig:{s}/usr/share/pkgconfig\" \\\n  CFLAGS=\"--sysroot={s} -I{s}/usr/include\" \\\n  LDFLAGS=\"--sysroot={s} -L{s}/usr/lib/amd64 -L{s}/usr/lib/64 -R/usr/lib/amd64 -R/usr/lib/64\" \\\n  OPENSSL_DIR=\"{s}/usr\" OPENSSL_NO_VENDOR=1 \\\n  cross build --target x86_64-unknown-illumos -p forged -p pkgdev"
    );

    Ok(())
}
