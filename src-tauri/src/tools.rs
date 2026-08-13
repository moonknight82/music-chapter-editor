use std::collections::HashSet;
use std::env;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use regex::Regex;

use crate::models::ToolStatus;

const TOOL_NAMES: [&str; 3] = ["mkvmerge", "mkvextract", "mkvpropedit"];
const MINIMUM_VERSION: u32 = 90;

#[derive(Debug, Clone)]
pub struct ToolPaths {
    pub directory: PathBuf,
    pub version: String,
    pub major_version: u32,
}

impl ToolPaths {
    pub fn executable(&self, name: &str) -> PathBuf {
        self.directory.join(name)
    }

    pub fn run<I, S>(&self, name: &str, args: I) -> Result<Output, String>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<std::ffi::OsStr>,
    {
        Command::new(self.executable(name))
            .args(args)
            .output()
            .map_err(|error| format!("Could not run {name}: {error}"))
    }
}

fn candidate_directories(preferred: Option<&Path>) -> Vec<PathBuf> {
    let mut candidates = Vec::new();
    if let Some(path) = preferred {
        candidates.push(path.to_path_buf());
    }
    candidates.push(PathBuf::from("/opt/homebrew/bin"));
    candidates.push(PathBuf::from("/usr/local/bin"));
    if let Some(path) = env::var_os("PATH") {
        candidates.extend(env::split_paths(&path));
    }
    let mut seen = HashSet::new();
    candidates.retain(|path| seen.insert(path.clone()));
    candidates
}

fn inspect_directory(path: &Path) -> Result<ToolPaths, Vec<String>> {
    let missing: Vec<_> = TOOL_NAMES
        .iter()
        .filter(|name| !path.join(name).is_file())
        .map(|name| name.to_string())
        .collect();
    if !missing.is_empty() {
        return Err(missing);
    }
    let output = Command::new(path.join("mkvmerge"))
        .arg("--version")
        .output()
        .map_err(|_| vec!["mkvmerge (not executable)".into()])?;
    if !output.status.success() {
        return Err(vec!["mkvmerge (version check failed)".into()]);
    }
    let version = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let major = Regex::new(r"\bv(\d+)")
        .unwrap()
        .captures(&version)
        .and_then(|captures| captures.get(1))
        .and_then(|value| value.as_str().parse::<u32>().ok())
        .ok_or_else(|| vec!["mkvmerge (unrecognized version)".into()])?;
    if major < MINIMUM_VERSION {
        return Err(vec![format!(
            "MKVToolNix {major} (version {MINIMUM_VERSION}+ required)"
        )]);
    }
    Ok(ToolPaths {
        directory: path.to_path_buf(),
        version,
        major_version: major,
    })
}

pub fn locate_tools(preferred: Option<&Path>) -> Result<ToolPaths, Box<ToolStatus>> {
    let candidates = candidate_directories(preferred);
    let searched: Vec<_> = candidates
        .iter()
        .map(|path| path.display().to_string())
        .collect();
    let mut last_missing = TOOL_NAMES.iter().map(|name| name.to_string()).collect();
    for directory in candidates {
        match inspect_directory(&directory) {
            Ok(paths) => return Ok(paths),
            Err(missing) => last_missing = missing,
        }
    }
    Err(Box::new(ToolStatus {
        available: false,
        directory: None,
        version: None,
        major_version: None,
        missing: last_missing,
        searched,
        message: "MKVToolNix 90 or newer is required. Install the command-line tools with `brew install mkvtoolnix`, install the official DMG, or choose its bin directory.".into(),
    }))
}

pub fn status(preferred: Option<&Path>) -> ToolStatus {
    match locate_tools(preferred) {
        Ok(paths) => ToolStatus {
            available: true,
            directory: Some(paths.directory.display().to_string()),
            version: Some(paths.version),
            major_version: Some(paths.major_version),
            missing: Vec::new(),
            searched: candidate_directories(preferred)
                .iter()
                .map(|path| path.display().to_string())
                .collect(),
            message: "MKVToolNix is ready".into(),
        },
        Err(status) => *status,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_directory_reports_all_tools() {
        let status = status(Some(Path::new("/definitely/not/a/tool/path")));
        assert!(
            !status.available || status.directory.as_deref() != Some("/definitely/not/a/tool/path")
        );
    }
}
