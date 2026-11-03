//! Pure channel‑tag logic for the bake registry (immutable‑deploy slice 5).
//!
//! Snapshots are immutable artifacts; channels (`unstable`/`latest`/`stable`) are
//! mutable pointers to them — the same model Docker uses for image tags. This
//! module holds the deterministic promotion decision; persistence lives in
//! `crate::db::bake_registry` and the actual image deletion in the Hetzner
//! connector.

/// Well‑known channel names. Stored as strings in the DB so the set is
/// extensible, but these are the ladder the promote convenience uses.
pub const CHANNEL_UNSTABLE: &str = "unstable";
pub const CHANNEL_LATEST: &str = "latest";
pub const CHANNEL_STABLE: &str = "stable";

/// Current snapshot ids each channel points to (bake_snapshot.id).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ChannelState {
    pub unstable: Option<i32>,
    pub latest: Option<i32>,
    pub stable: Option<i32>,
}

/// The pointer moves a promotion applies: `latest ← unstable`, `stable ← old
/// latest`. `unstable` is left as‑is (the next bake overwrites it).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PromoteMoves {
    pub new_latest: i32,
    pub new_stable: Option<i32>,
}

/// Roll the release ladder. Returns `None` (no‑op) when there is nothing to
/// promote — no `unstable` candidate, or it is already what `latest` points to.
pub fn promote_ladder(state: &ChannelState) -> Option<PromoteMoves> {
    match state.unstable {
        None => None,
        Some(u) if Some(u) == state.latest => None, // already promoted
        Some(u) => Some(PromoteMoves {
            new_latest: u,
            new_stable: state.latest,
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn promotes_unstable_to_latest_and_old_latest_to_stable() {
        let state = ChannelState {
            unstable: Some(3),
            latest: Some(2),
            stable: Some(1),
        };
        assert_eq!(
            promote_ladder(&state),
            Some(PromoteMoves {
                new_latest: 3,
                new_stable: Some(2)
            })
        );
    }

    #[test]
    fn first_promotion_with_no_prior_latest() {
        let state = ChannelState {
            unstable: Some(3),
            latest: None,
            stable: None,
        };
        assert_eq!(
            promote_ladder(&state),
            Some(PromoteMoves {
                new_latest: 3,
                new_stable: None
            })
        );
    }

    #[test]
    fn noop_when_nothing_to_promote() {
        assert_eq!(
            promote_ladder(&ChannelState {
                unstable: None,
                latest: Some(2),
                stable: Some(1)
            }),
            None
        );
    }

    #[test]
    fn noop_when_unstable_already_is_latest() {
        assert_eq!(
            promote_ladder(&ChannelState {
                unstable: Some(2),
                latest: Some(2),
                stable: Some(1)
            }),
            None
        );
    }
}
