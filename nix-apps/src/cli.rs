//! `nix-apps` commands. Both the `nix-apps` binary and the update checker
//! (`nixos-update-checker apps …`) run these.

use std::path::{Path, PathBuf};

use crate::appimage;
use crate::manifest::{id_from_name, AppImage, Manifest, Source};
use crate::source;
use crate::{Error, Result};

const USAGE: &str = "\
usage: nix-apps [--manifest FILE] COMMAND …

Commands:
  list                       what the manifest installs
  check [--json]             newer AppImage releases
  update [ID…]               move AppImages to their newest release (all when no ID)
  add ID --name NAME SOURCE  add an AppImage from a source (newest release)
  adopt FILE [--id ID] [SOURCE]
                             add a loose AppImage, its source read from the file
                             unless given; the file itself is left where it is
  remove ID                  remove an AppImage
  scan [DIR…] [--json]       loose AppImages and what they say about updates
                             (default: ~/Applications ~/AppImages ~/Downloads ~/.local/bin)

SOURCE is one of:
  --github OWNER/REPO [--asset GLOB] [--prerelease]
  --gitlab PROJECT [--host HOST] [--asset GLOB]
  --feed URL                 Electron latest-linux.yml
  --zsync URL
  --page URL --pattern REGEX
  --manual URL               pinned, never updated

The manifest defaults to /etc/nixos/apps.json ($NIX_APPS_MANIFEST overrides).
";

fn default_manifest() -> PathBuf {
    std::env::var_os("NIX_APPS_MANIFEST")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/etc/nixos/apps.json"))
}

struct Args {
    rest: Vec<String>,
}

impl Args {
    fn flag(&mut self, name: &str) -> bool {
        match self.rest.iter().position(|a| a == name) {
            Some(i) => {
                self.rest.remove(i);
                true
            }
            None => false,
        }
    }

    fn value(&mut self, name: &str) -> Result<Option<String>> {
        match self.rest.iter().position(|a| a == name) {
            Some(i) if i + 1 < self.rest.len() => {
                let v = self.rest.remove(i + 1);
                self.rest.remove(i);
                Ok(Some(v))
            }
            Some(_) => Err(Error(format!("{name} needs a value"))),
            None => Ok(None),
        }
    }

    /// A SOURCE given on the command line, if any.
    fn source(&mut self) -> Result<Option<(Source, Option<String>)>> {
        let asset = self.value("--asset")?.unwrap_or_else(|| "*.AppImage".into());
        if let Some(repo) = self.value("--github")? {
            let prerelease = self.flag("--prerelease");
            return Ok(Some((Source::Github { repo, asset, prerelease }, None)));
        }
        if let Some(project) = self.value("--gitlab")? {
            let host = self.value("--host")?.unwrap_or_else(|| "gitlab.com".into());
            return Ok(Some((Source::Gitlab { host, project, asset }, None)));
        }
        if let Some(feed) = self.value("--feed")? {
            return Ok(Some((Source::Electron { feed }, None)));
        }
        if let Some(url) = self.value("--zsync")? {
            return Ok(Some((Source::Zsync { url }, None)));
        }
        if let Some(url) = self.value("--page")? {
            let pattern = self.value("--pattern")?.ok_or_else(|| Error("--page needs --pattern".into()))?;
            return Ok(Some((Source::Page { url, pattern }, None)));
        }
        if let Some(url) = self.value("--manual")? {
            return Ok(Some((Source::Manual, Some(url))));
        }
        Ok(None)
    }

    fn done(&self) -> Result<()> {
        match self.rest.first() {
            Some(a) => Err(Error(format!("unexpected argument: {a}"))),
            None => Ok(()),
        }
    }
}

/// Runs a command line (without the program name). Returns the exit code.
pub fn run(args: Vec<String>) -> i32 {
    match dispatch(args) {
        Ok(()) => 0,
        Err(e) => {
            eprintln!("nix-apps: {e}");
            1
        }
    }
}

fn dispatch(args: Vec<String>) -> Result<()> {
    let mut a = Args { rest: args };
    if a.flag("--help") || a.flag("-h") || a.rest.is_empty() {
        print!("{USAGE}");
        return Ok(());
    }
    let manifest_path = a.value("--manifest")?.map(PathBuf::from).unwrap_or_else(default_manifest);
    let command = a.rest.remove(0);
    match command.as_str() {
        "list" => list(&manifest_path, a),
        "check" => check(&manifest_path, a),
        "update" => {
            let ids = std::mem::take(&mut a.rest);
            let updated = crate::apply(&manifest_path, &ids, |line| println!("{line}"))?;
            if updated.is_empty() {
                println!("All AppImages are up to date");
            }
            Ok(())
        }
        "add" => add(&manifest_path, a),
        "adopt" => adopt(&manifest_path, a),
        "remove" => {
            let id = a.rest.first().cloned().ok_or_else(|| Error("remove needs an ID".into()))?;
            a.rest.remove(0);
            a.done()?;
            let mut m = Manifest::load(&manifest_path)?;
            m.appimages.remove(&id).ok_or_else(|| Error(format!("{id}: not in the manifest")))?;
            m.save(&manifest_path)?;
            println!("Removed {id}; rebuild to uninstall it");
            Ok(())
        }
        "scan" => scan(a),
        other => Err(Error(format!("unknown command {other}; see --help"))),
    }
}

fn list(path: &Path, a: Args) -> Result<()> {
    a.done()?;
    let m = Manifest::load(path)?;
    for p in &m.nixpkgs {
        println!("nixpkgs   {p}");
    }
    for f in &m.flatpaks {
        println!("flatpak   {}", f.app_id());
    }
    for (id, app) in &m.appimages {
        println!("appimage  {id} {} ({})", app.version, app.source.describe());
    }
    Ok(())
}

fn check(path: &Path, mut a: Args) -> Result<()> {
    let json = a.flag("--json");
    a.done()?;
    let m = Manifest::load(path)?;
    let (updates, errors) = crate::check(&m);
    if json {
        let errors: Vec<_> = errors.iter().map(|(id, e)| serde_json::json!({"id": id, "error": e})).collect();
        println!("{}", serde_json::json!({ "updates": updates, "errors": errors }));
        return Ok(());
    }
    for u in &updates {
        println!("{} {} → {}", u.name, u.old_version, u.new_version);
    }
    for (id, e) in &errors {
        eprintln!("{id}: {e}");
    }
    if updates.is_empty() && errors.is_empty() {
        println!("All AppImages are up to date");
    }
    Ok(())
}

/// Pins the newest release of `source` as `id`.
fn pin(path: &Path, id: &str, name: &str, source: Source, manual_url: Option<String>) -> Result<()> {
    let mut m = Manifest::load(path)?;
    if m.appimages.contains_key(id) {
        return Err(Error(format!("{id} is already in the manifest")));
    }
    let release = match (&source, manual_url) {
        (Source::Manual, Some(url)) => {
            let (_, version) = appimage::name_and_version_from_file(url.rsplit('/').next().unwrap_or(&url));
            source::Release { version: version.unwrap_or_else(|| "0".into()), url, hash: None }
        }
        _ => source::latest(&source)?.ok_or_else(|| Error("no release to pin".into()))?,
    };
    println!("{name} {}: downloading {}…", release.version, release.url);
    let store_name = crate::store_name(id, &release.version);
    let hash = match &release.hash {
        Some(h) => source::prefetch(&release.url, &store_name, Some(h)).map(|_| h.clone())?,
        None => source::prefetch(&release.url, &store_name, None)?,
    };
    m.appimages.insert(
        id.to_string(),
        AppImage {
            name: name.to_string(),
            version: release.version.clone(),
            url: release.url,
            hash,
            source,
            extra_pkgs: Vec::new(),
            homepage: None,
        },
    );
    m.save(path)?;
    println!("Added {id} {} to {}; rebuild to install it", release.version, path.display());
    Ok(())
}

fn add(path: &Path, mut a: Args) -> Result<()> {
    let name = a.value("--name")?;
    let (source, manual) = a.source()?.ok_or_else(|| Error("add needs a SOURCE; see --help".into()))?;
    let id = a.rest.first().cloned().ok_or_else(|| Error("add needs an ID".into()))?;
    a.rest.remove(0);
    a.done()?;
    let name = name.unwrap_or_else(|| id.clone());
    pin(path, &id, &name, source, manual)
}

fn adopt(path: &Path, mut a: Args) -> Result<()> {
    let id = a.value("--id")?;
    let given = a.source()?;
    let file = a.rest.first().cloned().ok_or_else(|| Error("adopt needs a FILE".into()))?;
    a.rest.remove(0);
    a.done()?;
    let info = appimage::inspect(Path::new(&file))?;
    let (source, manual) = match given.or_else(|| info.source.clone().map(|s| (s, None))) {
        Some(s) => s,
        None => {
            return Err(Error(format!(
                "{} says nothing about where updates come from; give a SOURCE (see --help)",
                info.name
            )))
        }
    };
    let id = id.unwrap_or_else(|| id_from_name(&info.name));
    println!("{} ({}), updates from {}", info.name, info.version.as_deref().unwrap_or("unknown version"), source.describe());
    pin(path, &id, &info.name, source, manual)
}

fn scan(mut a: Args) -> Result<()> {
    let json = a.flag("--json");
    let dirs: Vec<PathBuf> = if a.rest.is_empty() {
        appimage::default_scan_dirs()
    } else {
        std::mem::take(&mut a.rest).into_iter().map(PathBuf::from).collect()
    };
    let mut out = Vec::new();
    for path in appimage::scan(&dirs) {
        match appimage::inspect(&path) {
            Ok(info) => {
                if json {
                    out.push(serde_json::json!({
                        "path": info.path, "id": info.id(), "name": info.name, "version": info.version,
                        "source": info.source, "updateInfo": info.update_info,
                    }));
                } else {
                    println!(
                        "{}\n  {} {}  {}",
                        path.display(),
                        info.name,
                        info.version.as_deref().unwrap_or("?"),
                        info.source.map(|s| format!("updates from {}", s.describe())).unwrap_or_else(|| "no update information".into())
                    );
                }
            }
            Err(e) => eprintln!("{e}"),
        }
    }
    if json {
        println!("{}", serde_json::Value::Array(out));
    }
    Ok(())
}
