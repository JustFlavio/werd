//! Parked folders: every Laravel project directly inside one becomes a site.
//!
//! The daemon rescans them regularly. New project folders appear as sites,
//! and sites whose folder was deleted, moved or unparked disappear. A site the
//! user linked by hand is never touched, even inside a parked folder.

use crate::projects;
use crate::settings::Settings;
use crate::state::State;
use anyhow::{bail, Context, Result};
use std::fs;
use std::path::Path;

/// Path as Werd stores it: canonical, without the `\\?\` prefix.
pub(crate) fn display_path(path: &Path) -> Result<String> {
    let canonical =
        fs::canonicalize(path).with_context(|| format!("Folder not found: {}", path.display()))?;
    Ok(canonical
        .to_string_lossy()
        .trim_start_matches(r"\\?\")
        .to_string())
}

pub(crate) fn is_laravel(path: &Path) -> bool {
    path.join("artisan").is_file() && path.join("composer.json").is_file()
}

pub(crate) fn add(root: &Path, state: &mut State, path: &str) -> Result<Vec<String>> {
    let folder = display_path(Path::new(path))?;
    if !Path::new(&folder).is_dir() {
        bail!("{path} is not a folder");
    }
    if is_laravel(Path::new(&folder)) {
        bail!("{folder} is a Laravel project; add it as a site, or park the folder that contains it");
    }
    let mut settings = Settings::load(root)?;
    if !settings.parked.contains(&folder) {
        settings.parked.push(folder);
        settings.parked.sort();
        settings.save(root)?;
    }
    scan(root, state)?;
    Ok(settings.parked)
}

pub(crate) fn remove(root: &Path, state: &mut State, path: &str) -> Result<Vec<String>> {
    let mut settings = Settings::load(root)?;
    let folder = display_path(Path::new(path)).unwrap_or_else(|_| path.to_string());
    let before = settings.parked.len();
    settings
        .parked
        .retain(|parked| !parked.eq_ignore_ascii_case(&folder) && parked != path);
    if settings.parked.len() == before {
        bail!("{path} is not parked");
    }
    settings.save(root)?;
    scan(root, state)?;
    Ok(settings.parked)
}

/// Adds new projects of parked folders and removes sites that left them.
/// Returns whether anything changed.
pub(crate) fn scan(root: &Path, state: &mut State) -> Result<bool> {
    let parked = Settings::load(root)?.parked;
    let mut changed = false;

    let gone: Vec<String> = state
        .projects
        .iter()
        .filter(|project| {
            project
                .parked
                .as_ref()
                .is_some_and(|folder| !parked.contains(folder) || !is_laravel(Path::new(&project.path)))
        })
        .map(|project| project.id.clone())
        .collect();
    for id in gone {
        projects::remove_parked(root, state, &id)?;
        changed = true;
    }

    for folder in &parked {
        let Ok(entries) = fs::read_dir(folder) else {
            continue;
        };
        let mut candidates: Vec<_> = entries
            .flatten()
            .map(|entry| entry.path())
            .filter(|path| path.is_dir() && is_laravel(path))
            .collect();
        candidates.sort();
        for candidate in candidates {
            let Ok(path) = display_path(&candidate) else {
                continue;
            };
            if state.projects.iter().any(|project| project.path == path) {
                continue;
            }
            if let Ok(project) = projects::add(root, state, &path) {
                let index = state.index(&project.id)?;
                state.projects[index].parked = Some(folder.clone());
                changed = true;
            }
        }
    }
    if changed {
        state.save(root)?;
    }
    Ok(changed)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn laravel(folder: &Path) {
        fs::create_dir_all(folder.join("public")).unwrap();
        fs::write(folder.join("artisan"), "").unwrap();
        fs::write(folder.join("composer.json"), "{}").unwrap();
    }

    #[test]
    fn parked_projects_come_and_go_with_their_folders() {
        let root = tempfile::tempdir().unwrap();
        let work = tempfile::tempdir().unwrap();
        laravel(&work.path().join("shop"));
        laravel(&work.path().join("blog"));
        fs::create_dir_all(work.path().join("notes")).unwrap();
        let mut state = State::default();

        let parks = add(root.path(), &mut state, &work.path().to_string_lossy()).unwrap();
        assert_eq!(parks.len(), 1);
        let mut names: Vec<_> = state.projects.iter().map(|p| p.name.clone()).collect();
        names.sort();
        assert_eq!(names, ["blog", "shop"]);
        assert!(state.projects.iter().all(|p| p.parked.is_some()));
        assert!(
            !scan(root.path(), &mut state).unwrap(),
            "a second scan changes nothing"
        );

        let first = state.projects[0].id.clone();
        let error = projects::remove(root.path(), &mut state, &first).unwrap_err();
        assert!(error.to_string().contains("parked"), "{error}");

        fs::remove_dir_all(work.path().join("blog")).unwrap();
        laravel(&work.path().join("api"));
        assert!(scan(root.path(), &mut state).unwrap());
        let mut names: Vec<_> = state.projects.iter().map(|p| p.name.clone()).collect();
        names.sort();
        assert_eq!(names, ["api", "shop"]);

        remove(root.path(), &mut state, &work.path().to_string_lossy()).unwrap();
        assert!(state.projects.is_empty());
    }

    #[test]
    fn manual_sites_inside_a_parked_folder_are_kept() {
        let root = tempfile::tempdir().unwrap();
        let work = tempfile::tempdir().unwrap();
        laravel(&work.path().join("shop"));
        let mut state = State::default();
        projects::add(
            root.path(),
            &mut state,
            &work.path().join("shop").to_string_lossy(),
        )
        .unwrap();
        add(root.path(), &mut state, &work.path().to_string_lossy()).unwrap();
        remove(root.path(), &mut state, &work.path().to_string_lossy()).unwrap();
        assert_eq!(state.projects.len(), 1);
        assert!(state.projects[0].parked.is_none());
    }

    #[test]
    fn a_project_folder_cannot_be_parked() {
        let root = tempfile::tempdir().unwrap();
        let work = tempfile::tempdir().unwrap();
        laravel(work.path());
        let mut state = State::default();
        assert!(add(root.path(), &mut state, &work.path().to_string_lossy()).is_err());
    }
}
