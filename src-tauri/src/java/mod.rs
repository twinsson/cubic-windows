pub mod runtime;

use std::path::{Path, PathBuf};
use std::process::Command;

use serde::{Deserialize, Serialize};

use crate::error::{AppError, AppResult};
use crate::paths::AppPaths;

pub use runtime::{ensure_java, ensure_java_for_game_version};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JavaRuntime {
    pub path: PathBuf,
    pub major_version: u32,
    pub vendor_hint: Option<String>,
}

pub fn find_java(required_major: u32, override_path: Option<&Path>) -> AppResult<JavaRuntime> {
    if let Some(path) = override_path {
        let runtime = inspect_java(path)?;
        if runtime.major_version >= required_major {
            return Ok(runtime);
        }
        return Err(AppError::msg(format!(
            "Configured Java at {} is major {}, need {}",
            path.display(),
            runtime.major_version,
            required_major
        )));
    }
    find_system_java(required_major)
}

pub fn find_system_java(required_major: u32) -> AppResult<JavaRuntime> {
    let mut runtimes = list_detected_java()?;
    runtimes.sort_by(|a, b| b.major_version.cmp(&a.major_version));
    runtimes
        .into_iter()
        .find(|rt| rt.major_version >= required_major)
        .ok_or(AppError::JavaNotFound {
            required: required_major,
        })
}

pub fn list_detected_java() -> AppResult<Vec<JavaRuntime>> {
    let mut candidates = Vec::new();
    if let Ok(java_home) = std::env::var("JAVA_HOME") {
        candidates.push(PathBuf::from(java_home).join("bin").join(AppPaths::java_bin_name()));
    }
    if let Ok(path) = which_java() {
        candidates.push(path);
    }
    candidates.extend(scan_system_jvms());
    candidates.extend(scan_managed_runtimes());

    let mut seen = std::collections::HashSet::new();
    let mut runtimes = Vec::new();
    for path in candidates {
        let Ok(canonical) = std::fs::canonicalize(&path) else {
            continue;
        };
        if !seen.insert(canonical.clone()) {
            continue;
        }
        if let Ok(rt) = inspect_java(&canonical) {
            runtimes.push(rt);
        }
    }
    runtimes.sort_by(|a, b| b.major_version.cmp(&a.major_version));
    Ok(runtimes)
}

fn scan_managed_runtimes() -> Vec<PathBuf> {
    let mut out = Vec::new();
    let bin = AppPaths::java_bin_name();
    if let Ok(paths) = AppPaths::resolve() {
        if let Ok(entries) = std::fs::read_dir(paths.runtime_dir()) {
            for entry in entries.flatten() {
                let java = entry.path().join("bin").join(bin);
                if java.is_file() {
                    out.push(java);
                }
            }
        }
    }

    // Common third-party launcher Java installs on Windows
    if let Ok(appdata) = std::env::var("APPDATA") {
        let roots = [
            PathBuf::from(&appdata).join("PrismLauncher\\java"),
            PathBuf::from(&appdata).join("PolyMC\\java"),
            PathBuf::from(&appdata).join(".minecraft\\runtime"),
        ];
        for root in roots {
            collect_java_bins(&root, bin, &mut out);
        }
    }
    if let Ok(local) = std::env::var("LOCALAPPDATA") {
        let roots = [
            PathBuf::from(&local).join("Packages\\Microsoft.4297127D64EC6_8wekyb3d8bbwe\\LocalCache\\Local\\runtime"),
            PathBuf::from(&local).join("PrismLauncher\\java"),
        ];
        for root in roots {
            collect_java_bins(&root, bin, &mut out);
        }
    }
    out
}

fn collect_java_bins(root: &Path, bin: &str, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(root) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let direct = path.join("bin").join(bin);
        if direct.is_file() {
            out.push(direct);
            continue;
        }
        // Official launcher layout: runtime/<component>/<os>/<component>/bin/java
        if let Ok(os_entries) = std::fs::read_dir(&path) {
            for os_entry in os_entries.flatten() {
                let nested = os_entry.path().join(path.file_name().unwrap_or_default()).join("bin").join(bin);
                if nested.is_file() {
                    out.push(nested);
                }
                let alt = os_entry.path().join("bin").join(bin);
                if alt.is_file() {
                    out.push(alt);
                }
            }
        }
    }
}

fn which_java() -> AppResult<PathBuf> {
    #[cfg(windows)]
    {
        let output = Command::new("where.exe").arg("java").output()?;
        if !output.status.success() {
            return Err(AppError::msg("`java` not found on PATH"));
        }
        let path = String::from_utf8_lossy(&output.stdout)
            .lines()
            .next()
            .unwrap_or("")
            .trim()
            .to_string();
        if path.is_empty() {
            return Err(AppError::msg("`java` not found on PATH"));
        }
        return Ok(PathBuf::from(path));
    }
    #[cfg(not(windows))]
    {
        let output = Command::new("which").arg("java").output()?;
        if !output.status.success() {
            return Err(AppError::msg("`java` not found on PATH"));
        }
        let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if path.is_empty() {
            return Err(AppError::msg("`java` not found on PATH"));
        }
        Ok(PathBuf::from(path))
    }
}

fn scan_system_jvms() -> Vec<PathBuf> {
    let bin = AppPaths::java_bin_name();
    let mut out = Vec::new();
    let mut roots = Vec::new();

    #[cfg(windows)]
    {
        for key in ["ProgramFiles", "ProgramFiles(x86)", "LOCALAPPDATA"] {
            if let Ok(base) = std::env::var(key) {
                let base = PathBuf::from(base);
                roots.push(base.join("Java"));
                roots.push(base.join("Eclipse Adoptium"));
                roots.push(base.join("Microsoft"));
                roots.push(base.join("Amazon Corretto"));
                roots.push(base.join("Zulu"));
            }
        }
    }
    #[cfg(not(windows))]
    {
        roots.extend([
            PathBuf::from("/usr/lib/jvm"),
            PathBuf::from("/usr/lib64/jvm"),
            PathBuf::from("/lib/jvm"),
            PathBuf::from("/usr/local/lib/jvm"),
        ]);
    }

    for root in roots {
        let Ok(entries) = std::fs::read_dir(root) else {
            continue;
        };
        for entry in entries.flatten() {
            let java = entry.path().join("bin").join(bin);
            if java.is_file() {
                out.push(java);
            }
        }
    }
    out
}

pub(crate) fn inspect_java(path: &Path) -> AppResult<JavaRuntime> {
    if let Some(from_release) = read_release_major(path) {
        return Ok(JavaRuntime {
            path: path.to_path_buf(),
            major_version: from_release,
            vendor_hint: None,
        });
    }

    let output = Command::new(path).arg("-version").output()?;
    let stderr = String::from_utf8_lossy(&output.stderr);
    let stdout = String::from_utf8_lossy(&output.stdout);
    let text = if stderr.trim().is_empty() {
        stdout
    } else {
        stderr
    };
    let major = parse_major_from_version_output(&text)
        .ok_or_else(|| AppError::msg(format!("Could not parse Java version from {}", path.display())))?;
    Ok(JavaRuntime {
        path: path.to_path_buf(),
        major_version: major,
        vendor_hint: None,
    })
}

fn read_release_major(java_bin: &Path) -> Option<u32> {
    let home = java_bin.parent()?.parent()?;
    let release = std::fs::read_to_string(home.join("release")).ok()?;
    for line in release.lines() {
        if let Some(rest) = line.strip_prefix("JAVA_VERSION=") {
            let ver = rest.trim().trim_matches('"');
            return parse_major_version_string(ver);
        }
    }
    None
}

fn parse_major_from_version_output(text: &str) -> Option<u32> {
    for line in text.lines() {
        if let Some(idx) = line.find('"') {
            let rest = &line[idx + 1..];
            if let Some(end) = rest.find('"') {
                return parse_major_version_string(&rest[..end]);
            }
        }
    }
    None
}

fn parse_major_version_string(ver: &str) -> Option<u32> {
    let mut parts = ver.split(['.', '-', '+', '_']);
    let first = parts.next()?;
    if first == "1" {
        parts.next()?.parse().ok()
    } else {
        first.parse().ok()
    }
}
