use std::collections::HashSet;

use bevy::prelude::warn;
use dd40_identity_core::AccessList;

/// Resolves an [`AccessList`] configuration value into a `HashSet<String>`.
///
/// - `Open` or `Inline([])` → empty set (caller interprets empty as "allow all").
/// - `Inline(subs)` → the set of those subs.
/// - `File(path)` → read the file, one `sub` per line (blank lines and
///   `#`-prefixed comment lines are skipped). Returns empty set on I/O error
///   (with a `warn!`).
pub fn resolve(list: &AccessList) -> HashSet<String> {
    match list {
        AccessList::Open => HashSet::new(),
        AccessList::Inline(subs) => subs.iter().cloned().collect(),
        AccessList::File(path) => match std::fs::read_to_string(path) {
            Ok(content) => content
                .lines()
                .map(str::trim)
                .filter(|l| !l.is_empty() && !l.starts_with('#'))
                .map(str::to_owned)
                .collect(),
            Err(e) => {
                warn!(path = %path.display(), error = %e, "failed to read access list file; treating as empty");
                HashSet::new()
            }
        },
    }
}

/// Returns `true` if `sub` is allowed to connect given `allow` and `deny` sets.
///
/// Deny is checked first; then allow (empty allow set = open to all).
pub fn is_allowed(sub: &str, allow: &HashSet<String>, deny: &HashSet<String>) -> bool {
    if deny.contains(sub) {
        return false;
    }
    allow.is_empty() || allow.contains(sub)
}

#[cfg(test)]
mod tests {
    use std::io::Write;

    use dd40_identity_core::AccessList;
    use tempfile::NamedTempFile;

    use super::*;

    #[test]
    fn open_resolves_to_empty() {
        let set = resolve(&AccessList::Open);
        assert!(set.is_empty());
    }

    #[test]
    fn empty_inline_resolves_to_empty() {
        let set = resolve(&AccessList::Inline(vec![]));
        assert!(set.is_empty());
    }

    #[test]
    fn inline_resolves_to_set() {
        let set = resolve(&AccessList::Inline(vec!["sub1".into(), "sub2".into()]));
        assert!(set.contains("sub1"));
        assert!(set.contains("sub2"));
        assert_eq!(set.len(), 2);
    }

    #[test]
    fn file_resolves_lines() {
        let mut file = NamedTempFile::new().unwrap();
        writeln!(file, "sub1").unwrap();
        writeln!(file, "# comment").unwrap();
        writeln!(file, "").unwrap();
        writeln!(file, "sub2").unwrap();
        let set = resolve(&AccessList::File(file.path().to_path_buf()));
        assert_eq!(set, HashSet::from(["sub1".into(), "sub2".into()]));
    }

    #[test]
    fn deny_beats_allow() {
        let allow = HashSet::from(["sub1".into()]);
        let deny = HashSet::from(["sub1".into()]);
        assert!(!is_allowed("sub1", &allow, &deny));
    }

    #[test]
    fn empty_allow_is_open() {
        let allow = HashSet::new();
        let deny = HashSet::new();
        assert!(is_allowed("anyone", &allow, &deny));
    }

    #[test]
    fn non_empty_allow_restricts() {
        let allow = HashSet::from(["sub1".into()]);
        let deny = HashSet::new();
        assert!(is_allowed("sub1", &allow, &deny));
        assert!(!is_allowed("sub2", &allow, &deny));
    }

    #[test]
    fn deny_blocks_regardless_of_allow() {
        let allow = HashSet::new(); // open
        let deny = HashSet::from(["bad_actor".into()]);
        assert!(!is_allowed("bad_actor", &allow, &deny));
        assert!(is_allowed("good_person", &allow, &deny));
    }
}
