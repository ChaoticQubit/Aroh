//! Last-write-wins rule shared by the device merge and the server engine.

/// The parts of a row the last-write-wins rule compares: its HLC string and
/// its tombstone flag.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Version {
    pub updated_at: String,
    pub deleted: bool,
}

/// Decides whether a remote row replaces the local one.
/// Input: the local version, if the row exists locally, and the remote version.
/// Output: true when the remote row must be written locally. An equal stamp
/// only wins when it turns a live row into a tombstone.
pub fn remote_wins(local: Option<&Version>, remote: &Version) -> bool {
    let Some(local) = local else { return true };
    match remote.updated_at.cmp(&local.updated_at) {
        std::cmp::Ordering::Greater => true,
        std::cmp::Ordering::Less => false,
        std::cmp::Ordering::Equal => remote.deleted && !local.deleted,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(updated_at: &str, deleted: bool) -> Version {
        Version { updated_at: updated_at.to_string(), deleted }
    }

    const OLD: &str = "1763683200000-0000-aaaaaaaaaaaaaaaa";
    const NEW: &str = "1763683200001-0000-bbbbbbbbbbbbbbbb";

    #[test]
    fn missing_local_row_takes_remote_including_tombstones() {
        assert!(remote_wins(None, &v(OLD, false)));
        assert!(remote_wins(None, &v(OLD, true)));
    }

    #[test]
    fn newer_remote_wins_and_older_remote_loses() {
        assert!(remote_wins(Some(&v(OLD, false)), &v(NEW, false)));
        assert!(!remote_wins(Some(&v(NEW, false)), &v(OLD, false)));
    }

    #[test]
    fn newer_tombstone_beats_older_edit() {
        assert!(remote_wins(Some(&v(OLD, false)), &v(NEW, true)));
    }

    #[test]
    fn edit_older_than_tombstone_does_not_resurrect() {
        assert!(!remote_wins(Some(&v(NEW, true)), &v(OLD, false)));
    }

    #[test]
    fn equal_hlc_prefers_tombstone_else_keeps_local() {
        assert!(remote_wins(Some(&v(OLD, false)), &v(OLD, true)));
        assert!(!remote_wins(Some(&v(OLD, true)), &v(OLD, false)));
        assert!(!remote_wins(Some(&v(OLD, false)), &v(OLD, false)));
    }

    #[test]
    fn merge_order_converges() {
        let writes = [v(OLD, false), v(NEW, false), v(OLD, true), v(NEW, true)];
        let apply = |order: &[usize]| {
            let mut state: Option<Version> = None;
            for &i in order {
                if remote_wins(state.as_ref(), &writes[i]) {
                    state = Some(writes[i].clone());
                }
            }
            state.unwrap()
        };
        let expected = apply(&[0, 1, 2, 3]);
        assert_eq!(expected, v(NEW, true));
        for order in [[3, 2, 1, 0], [1, 3, 0, 2], [2, 0, 3, 1], [0, 3, 1, 2]] {
            assert_eq!(apply(&order), expected);
        }
    }
}
