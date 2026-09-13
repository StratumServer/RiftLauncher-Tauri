//! Which paths the host will act on, and which it refuses.
//!
//! The rule is the one `src/ipc/pathPolicy.ts` enforced under Electron: a path
//! has to sit inside a root the saved config names. The config names five kinds
//! of root, the three configured folders plus the folder of every installation
//! and every game version it lists, and nothing outside those is the launcher's
//! to touch.
//!
//! Containment is checked twice, lexically and then again after resolving
//! symbolic links. The lexical answer alone is not enough: a link planted inside
//! a managed folder points wherever it likes, and then the answer the check gives
//! and the file the process actually opens stop being the same thing.

use std::path::{Component, Path, PathBuf};

/// Folds a path for comparison the way the TypeScript `comparablePath` does:
/// case-insensitively on Windows, exactly anywhere else.
fn comparable(path: &Path) -> String {
    let text = path.to_string_lossy().into_owned();
    if cfg!(windows) {
        text.to_lowercase()
    } else {
        text
    }
}

/// Resolves `.` and `..` without reading the filesystem.
///
/// Purely lexical on purpose: this runs before the path is known to exist, and
/// its only job is to stop `managed/../../etc` from reading as if it were still
/// under `managed`.
pub fn normalize(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                out.pop();
            }
            other => out.push(other.as_os_str()),
        }
    }
    out
}

/// True when `path` is the filesystem root, or a Windows drive root.
///
/// A root is never a managed root: granting one would grant the whole disk, which
/// is the check `assertNonRootPath` made on the Electron side.
pub fn is_root(path: &Path) -> bool {
    let normalized = normalize(path);
    normalized.parent().is_none() || normalized.components().count() <= 1
}

/// True when `candidate` is `root` or sits under it, lexically.
pub fn is_within(root: &Path, candidate: &Path) -> bool {
    if is_root(root) {
        return false;
    }

    let root = comparable(&normalize(root));
    let candidate = comparable(&normalize(candidate));

    if candidate == root {
        return true;
    }

    let separator = std::path::MAIN_SEPARATOR;
    let prefix = if root.ends_with(separator) {
        root
    } else {
        format!("{root}{separator}")
    };
    candidate.starts_with(&prefix)
}

/// Resolves symbolic links when the path exists, and answers the path itself when it does not.
fn resolved(path: &Path) -> PathBuf {
    path.canonicalize().unwrap_or_else(|_| normalize(path))
}

/// Whether `candidate` sits inside any of `roots`, before and after links are resolved.
///
/// Both passes have to agree. The lexical pass is what a config declaration is
/// checked against; the resolved pass is what stops a link inside a managed
/// folder from reaching out of it. A root that does not exist resolves to itself,
/// so a managed folder the player has not created yet still grants its subtree.
pub fn is_managed(roots: &[PathBuf], candidate: &Path) -> bool {
    let lexical = roots.iter().any(|root| is_within(root, candidate));
    if !lexical {
        return false;
    }

    let resolved_candidate = resolved(candidate);
    roots
        .iter()
        .any(|root| is_within(&resolved(root), &resolved_candidate))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn path(value: &str) -> PathBuf {
        PathBuf::from(value)
    }

    #[test]
    fn a_folder_grants_itself_and_its_subtree() {
        let root = path("/home/player/RiftLauncherInstallations");
        assert!(is_within(&root, &root));
        assert!(is_within(
            &root,
            &path("/home/player/RiftLauncherInstallations/survival")
        ));
        assert!(is_within(
            &root,
            &path("/home/player/RiftLauncherInstallations/survival/Mods/mod.zip")
        ));
    }

    #[test]
    fn a_sibling_that_shares_a_name_prefix_is_not_inside() {
        let root = path("/home/player/RiftLauncherInstallations");
        assert!(!is_within(
            &root,
            &path("/home/player/RiftLauncherInstallationsElsewhere")
        ));
    }

    #[test]
    fn parent_traversal_does_not_read_as_contained() {
        let root = path("/home/player/RiftLauncherInstallations");
        assert!(!is_within(
            &root,
            &path("/home/player/RiftLauncherInstallations/../../../etc/passwd")
        ));
    }

    #[test]
    fn the_filesystem_root_grants_nothing() {
        assert!(!is_within(&path("/"), &path("/etc/passwd")));
        assert!(is_root(&path("/")));
    }

    #[test]
    fn an_unrelated_path_is_outside_every_root() {
        let roots = vec![path("/home/player/versions"), path("/home/player/installs")];
        assert!(is_managed(&roots, &path("/home/player/versions/1.20.0")));
        assert!(!is_managed(&roots, &path("/tmp/elsewhere")));
    }
}
