use std::{
    fs,
    path::{Path, PathBuf},
};

const DATA_DIRECTORY: &str = "Chronolume";
const DATABASE_NAME: &str = "chronolume-v2.sqlite3";

#[cfg(target_os = "windows")]
const LEGACY_DATA_DIRECTORY: &str = "CodexUsage";
#[cfg(target_os = "windows")]
const LEGACY_DATABASE_NAME: &str = "codex-usage-v2.sqlite3";

pub const fn platform_id() -> &'static str {
    std::env::consts::OS
}

/// Resolves the derived analytics database under the operating system's data root.
/// The legacy brand migration is intentionally Windows-only because the old app never
/// shipped on macOS and probing a Windows namespace there would create false state.
pub fn resolve_analysis_database(local_data_root: &Path) -> std::io::Result<PathBuf> {
    let data_dir = local_data_root.join(DATA_DIRECTORY).join("v2");

    #[cfg(target_os = "windows")]
    migrate_windows_brand_data(local_data_root, &data_dir)?;

    fs::create_dir_all(&data_dir)?;
    let database_path = data_dir.join(DATABASE_NAME);

    #[cfg(target_os = "windows")]
    resume_windows_database_rename(&data_dir, &database_path)?;

    Ok(database_path)
}

/// Returns the filesystem identity used for hashing a workspace on macOS.
/// Existing paths are canonicalized read-only so symlinks and case aliases on the default
/// APFS configuration converge. Missing or inaccessible paths retain their exact normalized
/// spelling, which is the conservative behavior for case-sensitive volumes.
pub fn workspace_identity_path(normalized_display_path: &str) -> String {
    #[cfg(target_os = "macos")]
    {
        if normalized_display_path != "(unknown)" {
            if let Ok(canonical) = fs::canonicalize(normalized_display_path) {
                return normalize_path_text(&canonical.to_string_lossy());
            }
        }
    }

    normalized_display_path.to_string()
}

#[cfg(target_os = "macos")]
fn normalize_path_text(raw: &str) -> String {
    let replaced = raw.trim().to_string();
    let mut normalized = String::with_capacity(replaced.len());
    let mut previous_slash = false;
    for character in replaced.chars() {
        let slash = character == '/';
        if slash && previous_slash && !normalized.is_empty() {
            continue;
        }
        normalized.push(character);
        previous_slash = slash;
    }
    while normalized.len() > 1 && normalized.ends_with('/') {
        normalized.pop();
    }
    if normalized.is_empty() {
        "(unknown)".to_string()
    } else {
        normalized
    }
}

#[cfg(target_os = "windows")]
fn migrate_windows_brand_data(local_data_root: &Path, data_dir: &Path) -> std::io::Result<()> {
    let legacy_data_dir = local_data_root.join(LEGACY_DATA_DIRECTORY).join("v2");
    if !data_dir.exists() && legacy_data_dir.exists() {
        if let Some(parent) = data_dir.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::rename(legacy_data_dir, data_dir)?;
    }
    Ok(())
}

#[cfg(target_os = "windows")]
fn resume_windows_database_rename(data_dir: &Path, database_path: &Path) -> std::io::Result<()> {
    let legacy_database_path = data_dir.join(LEGACY_DATABASE_NAME);
    let migration_started = legacy_database_path.exists()
        || (database_path.exists()
            && ["-wal", "-shm"].iter().any(|suffix| {
                data_dir
                    .join(format!("{LEGACY_DATABASE_NAME}{suffix}"))
                    .exists()
            }));
    if migration_started {
        for suffix in ["", "-wal", "-shm"] {
            let legacy = data_dir.join(format!("{LEGACY_DATABASE_NAME}{suffix}"));
            let current = data_dir.join(format!("{DATABASE_NAME}{suffix}"));
            if legacy.exists() && !current.exists() {
                fs::rename(legacy, current)?;
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolves_current_platform_data_directory() {
        let root = tempfile::tempdir().expect("temporary root");
        let database = resolve_analysis_database(root.path()).expect("resolve database");
        assert_eq!(
            database,
            root.path().join("Chronolume/v2/chronolume-v2.sqlite3")
        );
    }

    #[test]
    fn missing_workspace_identity_preserves_unicode_spaces_and_case() {
        let path = "/Volumes/不存在/Project With Spaces/Δelta";
        assert_eq!(workspace_identity_path(path), path);
    }

    #[cfg(not(target_os = "windows"))]
    #[test]
    fn non_windows_platforms_never_probe_or_move_windows_brand_data() {
        let root = tempfile::tempdir().expect("temporary root");
        let legacy = root.path().join("CodexUsage/v2");
        fs::create_dir_all(&legacy).expect("legacy-shaped directory");
        fs::write(legacy.join("codex-usage-v2.sqlite3"), b"windows-only")
            .expect("legacy-shaped database");

        let database = resolve_analysis_database(root.path()).expect("resolve database");

        assert_eq!(
            database,
            root.path().join("Chronolume/v2/chronolume-v2.sqlite3")
        );
        assert!(legacy.join("codex-usage-v2.sqlite3").exists());
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn migrates_legacy_database_directory_and_wal_sidecars() {
        let root = tempfile::tempdir().expect("temporary root");
        let legacy = root.path().join(LEGACY_DATA_DIRECTORY).join("v2");
        fs::create_dir_all(&legacy).expect("legacy data directory");
        fs::write(legacy.join(LEGACY_DATABASE_NAME), b"database").expect("database");
        fs::write(legacy.join(format!("{LEGACY_DATABASE_NAME}-wal")), b"wal").expect("wal");
        fs::write(legacy.join(format!("{LEGACY_DATABASE_NAME}-shm")), b"shm").expect("shm");

        let database = resolve_analysis_database(root.path()).expect("migrate database");

        assert_eq!(fs::read(&database).expect("migrated database"), b"database");
        assert_eq!(
            fs::read(database.with_file_name(format!("{DATABASE_NAME}-wal")))
                .expect("migrated wal"),
            b"wal"
        );
        assert_eq!(
            fs::read(database.with_file_name(format!("{DATABASE_NAME}-shm")))
                .expect("migrated shm"),
            b"shm"
        );
        assert!(!legacy.exists());
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn keeps_existing_chronolume_database_authoritative() {
        let root = tempfile::tempdir().expect("temporary root");
        let current = root.path().join(DATA_DIRECTORY).join("v2");
        let legacy = root.path().join(LEGACY_DATA_DIRECTORY).join("v2");
        fs::create_dir_all(&current).expect("current data directory");
        fs::create_dir_all(&legacy).expect("legacy data directory");
        fs::write(current.join(DATABASE_NAME), b"current").expect("current database");
        fs::write(legacy.join(LEGACY_DATABASE_NAME), b"legacy").expect("legacy database");

        let database = resolve_analysis_database(root.path()).expect("resolve database");

        assert_eq!(fs::read(database).expect("current database"), b"current");
        assert!(legacy.join(LEGACY_DATABASE_NAME).exists());
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn resumes_after_the_main_database_was_already_renamed() {
        let root = tempfile::tempdir().expect("temporary root");
        let current = root.path().join(DATA_DIRECTORY).join("v2");
        fs::create_dir_all(&current).expect("current data directory");
        fs::write(current.join(DATABASE_NAME), b"database").expect("current database");
        fs::write(current.join(format!("{LEGACY_DATABASE_NAME}-wal")), b"wal").expect("legacy wal");

        let database = resolve_analysis_database(root.path()).expect("resume migration");

        assert_eq!(fs::read(&database).expect("database"), b"database");
        assert_eq!(
            fs::read(database.with_file_name(format!("{DATABASE_NAME}-wal")))
                .expect("migrated wal"),
            b"wal"
        );
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn canonicalizes_existing_paths_and_symlinks_without_writing_to_them() {
        use std::os::unix::fs::symlink;

        let root = tempfile::tempdir().expect("temporary root");
        let actual = root.path().join("Project With Spaces Δ");
        let alias = root.path().join("project-link");
        fs::create_dir(&actual).expect("workspace directory");
        symlink(&actual, &alias).expect("workspace symlink");

        let actual_identity = workspace_identity_path(&actual.to_string_lossy());
        let alias_identity = workspace_identity_path(&alias.to_string_lossy());
        assert_eq!(actual_identity, alias_identity);
        assert_eq!(
            fs::read_dir(&actual)
                .expect("workspace remains readable")
                .count(),
            0
        );
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn follows_volume_case_semantics_for_existing_paths() {
        let root = tempfile::tempdir().expect("temporary root");
        let actual = root.path().join("ActualCase");
        let alternate = root.path().join("actualcase");
        fs::create_dir(&actual).expect("workspace directory");

        let actual_identity = workspace_identity_path(&actual.to_string_lossy());
        let alternate_text = alternate.to_string_lossy();
        let alternate_identity = workspace_identity_path(&alternate_text);
        if alternate.exists() {
            assert_eq!(actual_identity, alternate_identity);
        } else {
            assert_ne!(actual_identity, alternate_identity);
        }
    }
}
