use anyhow::Result;
use rga::adapters::custom::map_exe_error;
use rga::adapters::*;
use rga::config::{RgaConfig, split_args};
use rga::matching::*;
use rga::print_dur;
use ripgrep_all as rga;
use clap::CommandFactory;

use schemars::schema_for;
use std::process::Command;
use std::time::Instant;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::{Semaphore, mpsc};
use tokio::task::JoinSet;
use tokio_rusqlite::Connection;
use sysinfo::{System, Pid};

fn list_adapters(args: RgaConfig) -> Result<()> {
    let (enabled_adapters, disabled_adapters) = get_all_adapters(args.custom_adapters);

    println!("Adapters:\n");
    let print = |adapter: std::sync::Arc<dyn FileAdapter>| {
        let meta = adapter.metadata();
        let matchers = meta
            .fast_matchers
            .iter()
            .map(|m| match m {
                FastFileMatcher::FileExtension(ext) => format!(".{ext}"),
            })
            .collect::<Vec<_>>()
            .join(", ");
        let slow_matchers = meta
            .slow_matchers
            .as_ref()
            .unwrap_or(&vec![])
            .iter()
            .filter_map(|m| match m {
                FileMatcher::MimeType(x) => Some(x.to_string()),
                FileMatcher::Fast(_) => None,
            })
            .collect::<Vec<_>>()
            .join(", ");
        print!(
            " - **{name}**\n     {desc}  \n     Extensions: {matchers}  \n     Mime Types: {mime}  \n",
            name = meta.name,
            desc = meta.description.replace('\n', "\n     "),
            matchers = matchers,
            mime = slow_matchers,
        );
        println!();
    };
    for adapter in enabled_adapters {
        print(adapter)
    }
    println!(
        "The following adapters are disabled by default, and can be enabled using '--rga-adapters=+foo,bar':\n"
    );
    for adapter in disabled_adapters {
        print(adapter)
    }
    Ok(())
}
fn main() -> anyhow::Result<()> {
    // set debugging as early as possible
    if std::env::args().any(|e| e == "--debug") {
        // TODO: Audit that the environment access only happens in single-threaded code.
        unsafe { std::env::set_var("RUST_LOG", "debug") };
    }

    env_logger::init();

    let (config, mut passthrough_args) = split_args(false)?;

    if config.print_config_schema {
        println!("{}", serde_json::to_string_pretty(&schema_for!(RgaConfig))?);
        return Ok(());
    }
    if config.list_adapters {
        return list_adapters(config);
    }
    if config.cache_prune {
        let max_bytes = if let Some(s) = &config.cache_prune_max_bytes { s.parse::<usize>().ok() } else { None };
        let ttl_days = config.cache_prune_ttl_days.unwrap_or(0);
        let rt = tokio::runtime::Builder::new_multi_thread().enable_all().build()?;
        rt.block_on(async {
            let dbpath = std::path::Path::new(&config.cache.path.0).join("cache.sqlite3");
            let db = Connection::open(dbpath).await?;
            if ttl_days > 0 {
                let ttl_ms: i64 = (ttl_days as i64) * 86400000;
                db.call(move |db| -> std::result::Result<(), rusqlite::Error> {
                    db.execute(
                        "delete from preproc_cache where created_unix_ms < (unixepoch()*1000 - ?)",
                        [ttl_ms],
                    )?;
                    Ok(())
                }).await?;
            }
            if let Some(maxb) = max_bytes {
                loop {
                    let (total, rows): (i64, i64) = db.call(|db| -> std::result::Result<(i64,i64), rusqlite::Error> {
                        let total: i64 = db.query_row(
                            "select coalesce(sum(length(text_content_zstd)),0) from preproc_cache",
                            [],
                            |r| r.get(0),
                        )?;
                        let rows: i64 = db.query_row(
                            "select count(*) from preproc_cache",
                            [],
                            |r| r.get(0),
                        )?;
                        Ok((total, rows))
                    }).await?;
                    if total as usize <= maxb || rows == 0 { break; }
                    let lim = 200i64;
                    db.call(move |db| -> std::result::Result<(), rusqlite::Error> {
                        db.execute(
                            "delete from preproc_cache where rowid in (select rowid from preproc_cache order by created_unix_ms asc limit ?)",
                            [lim],
                        )?;
                        Ok(())
                    }).await?;
                }
            }
            // optimize and vacuum after prune
            db.call(|db| -> std::result::Result<(), rusqlite::Error> {
                db.execute("PRAGMA optimize", [])?;
                Ok(())
            }).await?;
            db.call(|db| -> std::result::Result<(), rusqlite::Error> {
                db.execute("VACUUM", [])?;
                Ok(())
            }).await?;
            anyhow::Ok(())
        })?;
        return Ok(());
    }
    if config.cache_build {
        if let Some(p) = config.decompress_autotune_import.as_ref() { let _ = rga::adapters::decompress::import_caps_from_file(p); }
        let mut files: Vec<PathBuf> = Vec::new();
        let mut stack: Vec<PathBuf> = Vec::new();
        for p in passthrough_args.iter() { stack.push(PathBuf::from(p)); }
        while let Some(p) = stack.pop() {
            if let Ok(md) = std::fs::metadata(&p) {
                if md.is_file() { files.push(p); } else if md.is_dir() && let Ok(rd) = std::fs::read_dir(&p) {
                    for e in rd.flatten() { stack.push(e.path()); }
                }
            }
        }
        let total = files.len();
        let start = Instant::now();
        let rt = tokio::runtime::Builder::new_multi_thread().enable_all().build()?;
        rt.block_on(async {
            let concurrency = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4);
            let sem = Arc::new(Semaphore::new(concurrency));
            if let Some(max_rss) = config.max_rss_bytes {
                let sem2 = sem.clone();
                tokio::spawn(async move {
                    let mut sys = System::new();
                    let pid = std::process::id();
                    let mut held: Vec<tokio::sync::OwnedSemaphorePermit> = Vec::new();
                    loop {
                        sys.refresh_process(Pid::from_u32(pid));
                        if let Some(p) = sys.process(Pid::from_u32(pid)) {
                            let rss = p.memory().saturating_mul(1024);
                            if (rss as usize) > max_rss && held.len() < concurrency {
                                let p = sem2.clone().acquire_owned().await.unwrap();
                                held.push(p);
                            } else if (rss as usize) < (max_rss / 2) && !held.is_empty() {
                                let _ = held.pop();
                            }
                        }
                        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
                    }
                });
            }
            let (tx, mut rx) = mpsc::unbounded_channel::<()>();
            let mut js: JoinSet<anyhow::Result<()>> = JoinSet::new();
            for f in files.iter() {
                let sem2 = sem.clone();
                let tx2 = tx.clone();
                let cfg = config.clone();
                let f2 = f.clone();
                js.spawn(async move {
                    let _p = sem2.acquire_owned().await.unwrap();
                    let rd = tokio::fs::File::open(&f2).await?;
                    let ai = rga::adapters::AdaptInfo {
                        inp: Box::pin(rd),
                        filepath_hint: f2.clone(),
                        is_real_file: true,
                        line_prefix: "".to_string(),
                        archive_recursion_depth: 0,
                        postprocess: true,
                        config: cfg,
                    };
                    let mut r = rga::preproc::rga_preproc(ai).await?;
                    let mut s = tokio::io::sink();
                    let _ = tokio::io::copy(&mut r, &mut s).await?;
                    let _ = tx2.send(());
                    Ok(())
                });
            }
            let mut processed = 0usize;
            while let Some(res) = js.join_next().await {
                res??;
                while rx.try_recv().is_ok() {
                    processed += 1;
                    if processed / 100 != (processed.saturating_sub(1)) / 100 || processed == total {
                        println!("cache-build: {}/{} ({})", processed, total, print_dur(start));
                    }
                }
            }
            anyhow::Ok(())
        })?;
        println!("cache-build finished: {} files, {}", total, print_dur(start));
        if let Some(p) = config.decompress_autotune_export.as_ref() { let _ = rga::adapters::decompress::export_caps_to_file(p); }
        if config.profile { println!("{}", rga::preproc::prof_summary()); }
        return Ok(());
    }
    if config.doctor {
        add_exe_to_path()?;
        let check = |name: &str, args: &[&str]| -> (bool, String) {
            match Command::new(name).args(args).output() {
                Ok(out) => {
                    let mut s = String::new();
                    if !out.stdout.is_empty() { s.push_str(&String::from_utf8_lossy(&out.stdout)); }
                    if !out.stderr.is_empty() { s.push_str(&String::from_utf8_lossy(&out.stderr)); }
                    (true, s)
                }
                Err(_) => (false, String::new()),
            }
        };
        let items = vec![
            ("rg", vec!["--version"]),
            ("pdftotext", vec!["-v"]),
            ("pandoc", vec!["-v"]),
            ("ffprobe", vec!["-version"]),
            ("ffmpeg", vec!["-version"]),
        ];
        println!("Adapter Doctor:\n");
        for (name, args) in items.into_iter() {
            let (ok, ver) = check(name, &args);
            if ok { println!(" - {}: OK\n{}", name, ver.trim()); } else { println!(" - {}: MISSING", name); }
        }
        return Ok(());
    }
    if let Some(path) = config.fzf_path {
        if let Some(rest) = path.strip_prefix('_') {
            if rest.is_empty() {
                println!("[no file found]");
                return Ok(());
            }
            passthrough_args.push(std::ffi::OsString::from(rest));
        } else {
            passthrough_args.push(std::ffi::OsString::from(path));
        }
    }

    if passthrough_args.is_empty() {
        // rg would show help. Show own help instead.
        RgaConfig::command().print_help()?;
        println!();
        return Ok(());
    }

    let adapters = get_adapters_filtered(config.custom_adapters.clone(), &config.adapters)?;

    let pre_glob = if !config.accurate {
        let extensions = adapters
            .iter()
            .flat_map(|a| &a.metadata().fast_matchers)
            .flat_map(|m| match m {
                FastFileMatcher::FileExtension(ext) => vec![ext.clone(), ext.to_ascii_uppercase()],
            })
            .collect::<Vec<_>>()
            .join(",");
        format!("*.{{{extensions}}}")
    } else {
        "*".to_owned()
    };

    add_exe_to_path()?;

    let rg_args = vec![
        "--no-line-number",
        // smart case by default because within weird files
        // we probably can't really trust casing anyways
        "--smart-case",
    ];

    let exe = std::env::current_exe().expect("Could not get executable location");
    let preproc_exe = exe.with_file_name("rga-preproc");

    let before = Instant::now();
    let mut cmd = Command::new("rg");
    cmd.args(rg_args)
        .arg("--pre")
        .arg(preproc_exe)
        .arg("--pre-glob")
        .arg(pre_glob)
        .args(passthrough_args);
    if let Some(p) = config.decompress_autotune_import.as_ref() { cmd.env("RGA_DECOMPRESS_AUTOTUNE_IMPORT", p); }
    if let Some(p) = config.decompress_autotune_export.as_ref() { cmd.env("RGA_DECOMPRESS_AUTOTUNE_EXPORT", p); }
    log::debug!("rg command to run: {:?}", cmd);
    let mut child = cmd
        .spawn()
        .map_err(|e| map_exe_error(e, "rg", "Please make sure you have ripgrep installed."))?;

    let result = child.wait()?;

    log::debug!("running rg took {}", print_dur(before));
    if !result.success() {
        std::process::exit(result.code().unwrap_or(1));
    }
    Ok(())
}

/// add the directory that contains `rga` to PATH, so rga-preproc can find pandoc etc (if we are on Windows where we include dependent binaries)
fn add_exe_to_path() -> Result<()> {
    use std::env;
    let mut exe = env::current_exe().expect("Could not get executable location");
    // let preproc_exe = exe.with_file_name("rga-preproc");
    exe.pop(); // dirname

    let path = env::var_os("PATH").unwrap_or_default();
    let paths = env::split_paths(&path).collect::<Vec<_>>();
    // prepend: prefer bundled versions to system-installed versions of binaries
    // solves https://github.com/phiresky/ripgrep-all/issues/32
    // may be somewhat of a security issue if rga binary is in installed in unprivileged locations
    let paths = [&[exe.to_owned(), exe.join("lib")], &paths[..]].concat();
    let new_path = env::join_paths(paths)?;
    // TODO: Audit that the environment access only happens in single-threaded code.
    unsafe { env::set_var("PATH", new_path) };
    Ok(())
}
