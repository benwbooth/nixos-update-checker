//! Looking inside an AppImage file without running it. (Running one, even
//! with `--appimage-offset`, goes through binfmt and appimage-run on NixOS and
//! starts the app.)
//!
//! A type 2 AppImage is an ELF runtime followed by a SquashFS (or, for
//! uruntime builds, DwarFS) image. The runtime's `.upd_info` section holds
//! the update information AppImageUpdate reads; Electron apps also carry
//! `resources/app-update.yml`, their own updater's settings.

use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::manifest::{id_from_name, Source};
use crate::{Error, Result};

/// What a file says about itself.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Info {
    pub path: PathBuf,
    /// From the desktop entry (`Name=`), else the file name
    pub name: String,
    /// `X-AppImage-Version=`, else a version in the file name
    pub version: Option<String>,
    /// The raw `.upd_info` string ("zsync|https://…")
    pub update_info: Option<String>,
    /// Where updates come from, when the file says
    pub source: Option<Source>,
}

impl Info {
    pub fn id(&self) -> String {
        id_from_name(&self.name)
    }
}

/// Whether the file is an AppImage (magic "AI\x01"/"AI\x02" at byte 8).
pub fn is_appimage(path: &Path) -> bool {
    let mut head = [0u8; 12];
    File::open(path).and_then(|mut f| f.read_exact(&mut head)).is_ok()
        && &head[0..4] == b"\x7fELF"
        && &head[8..10] == b"AI"
        && matches!(head[10], 1 | 2)
}

struct Elf {
    /// End of the ELF runtime: where the filesystem image starts
    end: u64,
    update_info: Option<String>,
}

fn read_elf(f: &mut File) -> Result<Elf> {
    let mut h = [0u8; 64];
    f.seek(SeekFrom::Start(0))?;
    f.read_exact(&mut h)?;
    if &h[0..4] != b"\x7fELF" || h[4] != 2 || h[5] != 1 {
        return Err(Error("not a 64-bit little-endian ELF".into()));
    }
    let u16_at = |b: &[u8], o: usize| u16::from_le_bytes([b[o], b[o + 1]]) as u64;
    let u32_at = |b: &[u8], o: usize| u32::from_le_bytes(b[o..o + 4].try_into().unwrap()) as u64;
    let u64_at = |b: &[u8], o: usize| u64::from_le_bytes(b[o..o + 8].try_into().unwrap());
    let shoff = u64_at(&h, 0x28);
    let shentsize = u16_at(&h, 0x3A);
    let shnum = u16_at(&h, 0x3C);
    let shstrndx = u16_at(&h, 0x3E);
    let end = shoff + shentsize * shnum;
    if shentsize < 64 || shnum == 0 || shnum > 4096 || shstrndx >= shnum {
        return Ok(Elf { end, update_info: None });
    }
    let mut table = vec![0u8; (shentsize * shnum) as usize];
    f.seek(SeekFrom::Start(shoff))?;
    f.read_exact(&mut table)?;
    let sh = |i: u64| &table[(i * shentsize) as usize..((i + 1) * shentsize) as usize];
    let read_at = |f: &mut File, off: u64, len: u64| -> Result<Vec<u8>> {
        let mut buf = vec![0u8; len.min(1 << 20) as usize];
        f.seek(SeekFrom::Start(off))?;
        f.read_exact(&mut buf)?;
        Ok(buf)
    };
    let strtab = read_at(f, u64_at(sh(shstrndx), 0x18), u64_at(sh(shstrndx), 0x20))?;
    let mut update_info = None;
    for i in 0..shnum {
        let s = sh(i);
        let name_off = u32_at(s, 0) as usize;
        let name = strtab.get(name_off..).and_then(|b| b.split(|&c| c == 0).next()).unwrap_or_default();
        if name == b".upd_info" {
            let data = read_at(f, u64_at(s, 0x18), u64_at(s, 0x20))?;
            let text = String::from_utf8_lossy(data.split(|&c| c == 0).next().unwrap_or_default()).trim().to_string();
            update_info = (!text.is_empty()).then_some(text);
        }
    }
    Ok(Elf { end, update_info })
}

/// The update information embedded in the runtime, if any.
pub fn update_info(path: &Path) -> Result<Option<String>> {
    Ok(read_elf(&mut File::open(path)?)?.update_info)
}

/// A source from AppImage update information:
/// `zsync|<url>` or `gh-releases-zsync|<owner>|<repo>|<tag>|<file glob>.zsync`.
pub fn source_from_update_info(info: &str) -> Option<Source> {
    let parts: Vec<&str> = info.split('|').collect();
    match parts.as_slice() {
        ["zsync", url] => Some(Source::Zsync { url: url.to_string() }),
        ["gh-releases-zsync", owner, repo, tag, glob] => Some(Source::Github {
            repo: format!("{owner}/{repo}"),
            asset: glob.strip_suffix(".zsync").unwrap_or(glob).to_string(),
            prerelease: *tag == "latest-pre",
        }),
        _ => None,
    }
}

/// A source from Electron's app-update.yml (electron-builder's publish
/// settings): `provider: github` (owner, repo) or `provider: generic` (url,
/// optional channel).
pub fn source_from_app_update_yml(yml: &str) -> Option<Source> {
    let get = |key: &str| {
        yml.lines().find_map(|l| {
            l.strip_prefix(key)
                .and_then(|r| r.strip_prefix(':'))
                .map(|v| v.trim().trim_matches(['\'', '"']).to_string())
        })
    };
    match get("provider")?.as_str() {
        "github" => Some(Source::Github {
            repo: format!("{}/{}", get("owner")?, get("repo")?),
            asset: "*.AppImage".into(),
            prerelease: false,
        }),
        "generic" => {
            let url = get("url")?;
            let channel = get("channel").unwrap_or_else(|| "latest".into());
            Some(Source::Electron { feed: format!("{}/{channel}-linux.yml", url.trim_end_matches('/')) })
        }
        _ => None,
    }
}

/// A file from the SquashFS image, through unsquashfs. None when it isn't
/// there or the image isn't SquashFS.
fn squashfs_cat(path: &Path, offset: u64, inner: &str) -> Option<String> {
    let out = Command::new("unsquashfs")
        .args(["-o", &offset.to_string(), "-cat"])
        .arg(path)
        .arg(inner)
        .output()
        .ok()?;
    (out.status.success() && !out.stdout.is_empty()).then(|| String::from_utf8_lossy(&out.stdout).into_owned())
}

fn squashfs_top_level(path: &Path, offset: u64) -> Vec<String> {
    let Ok(out) = Command::new("unsquashfs").args(["-o", &offset.to_string(), "-l"]).arg(path).output() else {
        return Vec::new();
    };
    // Paths are listed under the extraction folder: "squashfs-root/app.desktop".
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .filter_map(|l| l.strip_prefix("squashfs-root/"))
        .filter(|l| !l.is_empty() && !l.contains('/'))
        .map(str::to_string)
        .collect()
}

fn desktop_value(desktop: &str, key: &str) -> Option<String> {
    let mut in_entry = false;
    for line in desktop.lines() {
        let line = line.trim();
        if line.starts_with('[') {
            in_entry = line == "[Desktop Entry]";
        } else if in_entry {
            if let Some(v) = line.strip_prefix(key).and_then(|r| r.strip_prefix('=')) {
                return Some(v.trim().to_string());
            }
        }
    }
    None
}

/// "Redact-0.21.34.AppImage" → ("Redact", Some("0.21.34"))
pub fn name_and_version_from_file(file: &str) -> (String, Option<String>) {
    let stem = file.rsplit_once('.').map(|(s, _)| s).unwrap_or(file);
    let re = regex::Regex::new(r"[-_ ]v?(\d+(?:\.\d+)+[0-9A-Za-z.+~-]*?)(?:[-_.](?:x86_64|amd64|x64|linux|SteamDeck).*)?$").unwrap();
    match re.captures(stem) {
        Some(c) => {
            let name = stem[..c.get(0).unwrap().start()].replace(['_'], " ");
            (name.trim().to_string(), Some(c[1].to_string()))
        }
        None => {
            let name = stem
                .split(['-', '_'])
                .take_while(|p| !matches!(p.to_lowercase().as_str(), "x86" | "x86_64" | "amd64" | "x64" | "linux"))
                .collect::<Vec<_>>()
                .join(" ");
            (if name.is_empty() { stem.to_string() } else { name }, None)
        }
    }
}

/// Everything the file says about itself. Reads the runtime directly and the
/// image through unsquashfs; never executes the file.
pub fn inspect(path: &Path) -> Result<Info> {
    if !is_appimage(path) {
        return Err(Error(format!("{}: not an AppImage", path.display())));
    }
    let mut f = File::open(path)?;
    let elf = read_elf(&mut f)?;
    let mut magic = [0u8; 4];
    f.seek(SeekFrom::Start(elf.end))?;
    let squashfs = f.read_exact(&mut magic).is_ok() && &magic == b"hsqs";

    let file = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let (file_name, file_version) = name_and_version_from_file(&file);
    let mut info = Info {
        path: path.to_path_buf(),
        name: file_name,
        version: file_version,
        update_info: elf.update_info.clone(),
        source: elf.update_info.as_deref().and_then(source_from_update_info),
    };
    if squashfs {
        if let Some(desktop) = squashfs_top_level(path, elf.end)
            .iter()
            .find(|n| n.ends_with(".desktop"))
            .and_then(|n| squashfs_cat(path, elf.end, n))
        {
            if let Some(name) = desktop_value(&desktop, "Name") {
                info.name = name;
            }
            if let Some(v) = desktop_value(&desktop, "X-AppImage-Version") {
                info.version = Some(v);
            }
        }
        if info.source.is_none() {
            info.source = squashfs_cat(path, elf.end, "resources/app-update.yml")
                .as_deref()
                .and_then(source_from_app_update_yml);
        }
    }
    Ok(info)
}

/// AppImages directly in `dirs` (not below), skipping what isn't one.
pub fn scan(dirs: &[PathBuf]) -> Vec<PathBuf> {
    let mut found = Vec::new();
    for dir in dirs {
        let Ok(entries) = std::fs::read_dir(dir) else { continue };
        for e in entries.flatten() {
            let p = e.path();
            let is_file = std::fs::metadata(&p).map(|m| m.is_file()).unwrap_or(false);
            let named = p.extension().is_some_and(|x| x.eq_ignore_ascii_case("appimage"));
            if is_file && named && is_appimage(&p) {
                found.push(p);
            }
        }
    }
    found.sort();
    found
}

/// Where loose AppImages usually are.
pub fn default_scan_dirs() -> Vec<PathBuf> {
    let home = std::env::var_os("HOME").map(PathBuf::from).unwrap_or_default();
    ["Applications", "AppImages", "Downloads", ".local/bin"].iter().map(|d| home.join(d)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn update_info_sources() {
        assert_eq!(
            source_from_update_info("zsync|https://downloads.cursor.com/x/cursor-latest.appimage.zsync"),
            Some(Source::Zsync { url: "https://downloads.cursor.com/x/cursor-latest.appimage.zsync".into() })
        );
        assert_eq!(
            source_from_update_info("gh-releases-zsync|probonopd|Etcher|latest|Etcher-*x86_64.AppImage.zsync"),
            Some(Source::Github { repo: "probonopd/Etcher".into(), asset: "Etcher-*x86_64.AppImage".into(), prerelease: false })
        );
        assert_eq!(source_from_update_info("bintray-zsync|a|b|c|d"), None);
    }

    #[test]
    fn app_update_yml() {
        let generic = "provider: generic\nurl: https://update-desktop.redact.dev/build\nuseMultipleRangeRequest: false\n";
        assert_eq!(
            source_from_app_update_yml(generic),
            Some(Source::Electron { feed: "https://update-desktop.redact.dev/build/latest-linux.yml".into() })
        );
        let gh = "owner: SteamGridDB\nrepo: steam-rom-manager\nprovider: github\nupdaterCacheDirName: x\n";
        assert_eq!(
            source_from_app_update_yml(gh),
            Some(Source::Github { repo: "SteamGridDB/steam-rom-manager".into(), asset: "*.AppImage".into(), prerelease: false })
        );
    }

    #[test]
    fn file_names() {
        assert_eq!(name_and_version_from_file("Redact-0.21.34.AppImage"), ("Redact".into(), Some("0.21.34".into())));
        assert_eq!(name_and_version_from_file("Cursor-3.2.16-x86_64.appimage"), ("Cursor".into(), Some("3.2.16".into())));
        assert_eq!(name_and_version_from_file("altirraqt-v0.1.0-x86_64.AppImage"), ("altirraqt".into(), Some("0.1.0".into())));
        assert_eq!(name_and_version_from_file("Steam ROM Manager.AppImage"), ("Steam ROM Manager".into(), None));
        assert_eq!(
            name_and_version_from_file("EmulationStation-DE-x64_SteamDeck.AppImage"),
            ("EmulationStation DE".into(), None)
        );
    }

    #[test]
    fn desktop_entries() {
        let d = "[Desktop Entry]\nName=Redact\nX-AppImage-Version=0.21.34\n[Desktop Action x]\nName=Other\n";
        assert_eq!(desktop_value(d, "Name").as_deref(), Some("Redact"));
        assert_eq!(desktop_value(d, "X-AppImage-Version").as_deref(), Some("0.21.34"));
    }
}
