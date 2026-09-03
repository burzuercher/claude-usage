//! Discovers Claude "environments" — separate config directories, one per
//! account. The user may keep several side by side, e.g. `~/.claude` (default),
//! `~/.claude-max` (personal Max), `~/.claude-team` (work Team). Each has its
//! own `projects/` logs and its own `.claude.json` account file, so we can list
//! them and let the widget toggle between accounts.

use serde::Serialize;
use std::path::{Path, PathBuf};

use crate::account::{read_account_at, Account};

pub const DEFAULT_ID: &str = ".claude";

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Environment {
    pub id: String,    // directory name, e.g. ".claude-team"
    pub label: String, // org name / email / id for display
    pub account: Account,
    pub has_logs: bool,
}

/// The account file for an environment: `<dir>/.claude.json` if present,
/// otherwise the home-level `~/.claude.json` (used by the default `.claude`).
fn account_file_for(dir: &Path, id: &str, home: &Path) -> PathBuf {
    let inner = dir.join(".claude.json");
    if inner.exists() {
        inner
    } else if id == DEFAULT_ID {
        home.join(".claude.json")
    } else {
        inner // non-existent; read_account_at returns default
    }
}

fn projects_dir(dir: &Path) -> PathBuf {
    dir.join("projects")
}

/// All environments: `~/.claude*` directories that contain a `projects/` folder.
pub fn list() -> Vec<Environment> {
    let home = match dirs::home_dir() {
        Some(h) => h,
        None => return Vec::new(),
    };
    let mut envs: Vec<Environment> = Vec::new();
    if let Ok(rd) = std::fs::read_dir(&home) {
        for entry in rd.filter_map(|e| e.ok()) {
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }
            let name = match entry.file_name().into_string() {
                Ok(n) => n,
                Err(_) => continue,
            };
            if !name.starts_with(".claude") {
                continue;
            }
            let pdir = projects_dir(&path);
            if !pdir.exists() {
                continue; // not an account env (e.g. .claude-monitor tool dir)
            }
            let has_logs = std::fs::read_dir(&pdir)
                .map(|mut rd| rd.any(|_| true))
                .unwrap_or(false);
            let account = read_account_at(&account_file_for(&path, &name, &home));
            let label = if !account.org_name.is_empty() {
                account.org_name.clone()
            } else if !account.email.is_empty() {
                account.email.clone()
            } else {
                name.clone()
            };
            envs.push(Environment {
                id: name,
                label,
                account,
                has_logs,
            });
        }
    }
    // Default env first, then by id.
    envs.sort_by(|a, b| {
        let ka = (a.id != DEFAULT_ID, a.id.clone());
        let kb = (b.id != DEFAULT_ID, b.id.clone());
        ka.cmp(&kb)
    });
    envs
}

/// Resolve an environment id to its config directory. The id is either a plain
/// `.claude*` directory name under home, or — for manually-added environments —
/// an absolute path to a Claude config dir (or its `projects/` dir).
pub fn config_dir_for(env_id: &str) -> Option<PathBuf> {
    let id = if env_id.is_empty() { DEFAULT_ID } else { env_id };
    let p = Path::new(id);
    if p.is_absolute() {
        // User-provided custom path.
        if p.exists() {
            return Some(p.to_path_buf());
        }
        return None;
    }
    // Plain name under home — guard against path traversal.
    if !id.starts_with(".claude") || id.contains('/') || id.contains('\\') || id.contains("..") {
        return None;
    }
    let dir = dirs::home_dir()?.join(id);
    if dir.exists() {
        Some(dir)
    } else {
        None
    }
}

/// Resolve an environment id to its `projects/` directory (accepts a dir that is
/// already a `projects/` folder for custom paths).
pub fn projects_dir_for(env_id: &str) -> Option<PathBuf> {
    let dir = config_dir_for(env_id)?;
    let pdir = projects_dir(&dir);
    if pdir.exists() {
        Some(pdir)
    } else if dir.file_name().map(|n| n == "projects").unwrap_or(false) {
        Some(dir)
    } else {
        None
    }
}

/// The account file (`.claude.json`) for any environment id. Besides the
/// `oauthAccount`, Claude Code keeps its cached usage response and feature
/// flags in this file, which `realusage` reads.
pub fn account_file_for_env(env_id: &str) -> Option<PathBuf> {
    let dir = config_dir_for(env_id)?;
    let home = dirs::home_dir().unwrap_or_default();
    let id = if env_id.is_empty() { DEFAULT_ID } else { env_id };
    Some(account_file_for(&dir, id, &home))
}

/// Read the account for any environment id (discovered name or custom path).
pub fn account_for(env_id: &str) -> Account {
    match account_file_for_env(env_id) {
        Some(p) => read_account_at(&p),
        None => Account::default(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_traversal_and_non_claude_names() {
        assert!(config_dir_for("../etc").is_none());
        assert!(config_dir_for(".claude/../secret").is_none()); // contains '/'
        assert!(config_dir_for("notclaude").is_none());
    }

    #[test]
    fn resolves_existing_absolute_path() {
        let tmp = std::env::temp_dir();
        assert_eq!(config_dir_for(tmp.to_str().unwrap()), Some(tmp.clone()));
        assert!(config_dir_for("C:/definitely/missing/path/xyz123").is_none());
    }

    #[test]
    fn projects_dir_accepts_a_projects_folder() {
        let dir = std::env::temp_dir().join(format!("cu-env-{}", std::process::id()));
        let projects = dir.join("projects");
        std::fs::create_dir_all(&projects).unwrap();
        // The config dir resolves to its projects/ subdir.
        assert_eq!(projects_dir_for(dir.to_str().unwrap()), Some(projects.clone()));
        // A path that already points at a projects/ folder is accepted as-is.
        assert_eq!(projects_dir_for(projects.to_str().unwrap()), Some(projects.clone()));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
