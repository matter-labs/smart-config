use std::{
    cmp::Ordering,
    collections::{BTreeSet, HashMap},
};

use crate::{metadata::BasicTypes, value::Pointer};

/// Mounting point info sufficient to resolve the mounted config / param.
// TODO: add refs
#[derive(Debug, Clone)]
pub(super) enum MountingPoint {
    /// Contains type IDs of mounted config(s).
    Config,
    Param {
        is_canonical: bool,
        expecting: BasicTypes,
    },
}

/// Wrapper for object paths that orders paths first by the key-value path (i.e., a path with all `.`s replaced
/// with `_`s), and then using ordinary lexicographical order. This allows to efficiently perform lookups
/// by key-value paths, such as [`MountingPoints::by_kv_path()`].
#[derive(Debug, Clone, PartialEq, Eq)]
struct KvPath(String);

impl From<&str> for KvPath {
    fn from(value: &str) -> Self {
        Self(value.into())
    }
}

impl PartialOrd for KvPath {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl KvPath {
    fn is_equivalent(&self, kv_path: &str) -> bool {
        if kv_path.len() != self.0.len() {
            return false;
        }

        Self::cmp_with_substitutions(&self.0, kv_path, &mut Ordering::Equal).is_eq()
    }

    fn cmp_with_substitutions(this: &str, other: &str, total_ordering: &mut Ordering) -> Ordering {
        for (&this_byte, &other_byte) in this.as_bytes().iter().zip(other.as_bytes()) {
            let this_byte = if this_byte == b'.' {
                *total_ordering = total_ordering.then(Ordering::Less); // because '.' < '_' and we substitute it in `self`
                b'_'
            } else {
                this_byte
            };
            let other_byte = if other_byte == b'.' {
                *total_ordering = total_ordering.then(Ordering::Greater);
                b'_'
            } else {
                other_byte
            };
            let compared = this_byte.cmp(&other_byte);
            if compared != Ordering::Equal {
                return compared;
            }
        }

        // If we've reached this point, the common part is identical after `.` -> `_` substitution.
        this.len().cmp(&other.len())
    }
}

impl Ord for KvPath {
    fn cmp(&self, other: &Self) -> Ordering {
        let mut total_ordering = Ordering::Equal;
        Self::cmp_with_substitutions(&self.0, &other.0, &mut total_ordering).then(total_ordering)
    }
}

#[derive(Debug, Clone, Default)]
pub(super) struct MountingPoints {
    kv_paths: BTreeSet<KvPath>,
    inner: HashMap<String, MountingPoint>,
}

impl MountingPoints {
    pub(super) fn get(&self, path: &str) -> Option<&MountingPoint> {
        self.inner.get(path)
    }

    pub(super) fn by_kv_path<'s>(
        &'s self,
        kv_path: &'s str,
    ) -> impl Iterator<Item = (Pointer<'s>, &'s MountingPoint)> + 's {
        let kv_paths = self
            .kv_paths
            // KV path is lexicographically greatest among all equivalent paths, hence using it as the upper bound
            // and `rev()` below
            .range(..=KvPath::from(kv_path))
            .rev()
            .take_while(|&path| path.is_equivalent(kv_path));
        kv_paths.map(|path| (Pointer(&path.0), &self.inner[&path.0]))
    }

    pub(super) fn insert(&mut self, path: String, mount: MountingPoint) {
        self.kv_paths.insert(KvPath(path.clone()));
        self.inner.insert(path, mount);
    }

    pub(super) fn extend(&mut self, mut from: Self) {
        self.kv_paths.append(&mut from.kv_paths);
        self.inner.extend(from.inner);
    }
}

#[cfg(test)]
mod tests {
    use std::{collections::HashSet, ops};

    use super::*;

    impl ops::Index<&str> for MountingPoints {
        type Output = MountingPoint;

        fn index(&self, index: &str) -> &Self::Output {
            self.get(index)
                .unwrap_or_else(|| panic!("no mounting point at {index:?}"))
        }
    }

    #[test]
    fn kv_path_ordering() {
        assert_eq!(
            KvPath::from("test").cmp(&KvPath::from("test")),
            Ordering::Equal
        );
        assert!(KvPath::from("test") < KvPath::from("test0"));
        assert!(KvPath::from("test0") > KvPath::from("test"));
        assert!(KvPath::from("test.value") < KvPath::from("test_value"));
        assert!(KvPath::from("test_value") > KvPath::from("test.value"));
        assert!(KvPath::from("test.value") > KvPath::from("test0value"));
        assert!(KvPath::from("test_value") > KvPath::from("test0value"));
    }

    #[test]
    fn kv_path_equivalence() {
        assert!(KvPath::from("test").is_equivalent("test"));
        assert!(KvPath::from("test.path").is_equivalent("test_path"));
        assert!(KvPath::from("test_path").is_equivalent("test_path"));
        assert!(!KvPath::from("test_path").is_equivalent("test"));
        assert!(!KvPath::from("test.path").is_equivalent("test"));
    }

    #[test]
    fn getting_mounting_points_by_kv_path() {
        let mut points = MountingPoints::default();
        let mount = MountingPoint::Param {
            expecting: BasicTypes::BOOL,
            is_canonical: true,
        };
        points.insert("test_path".into(), mount.clone());
        points.insert("test.path".into(), mount.clone());
        points.insert("test".into(), mount.clone());
        points.insert("path".into(), mount.clone());
        points.insert("testpath".into(), mount.clone());
        points.insert("test.path_1".into(), mount);

        let paths: HashSet<_> = points
            .by_kv_path("test_path")
            .map(|(path, _)| path)
            .collect();
        assert_eq!(
            paths,
            HashSet::from([Pointer("test_path"), Pointer("test.path")])
        );

        let paths: HashSet<_> = points
            .by_kv_path("test_path_1")
            .map(|(path, _)| path)
            .collect();
        assert_eq!(paths, HashSet::from([Pointer("test.path_1")]));
    }
}
