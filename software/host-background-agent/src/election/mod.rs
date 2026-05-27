use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant};

const PEER_TIMEOUT: Duration = Duration::from_secs(10);

#[derive(Clone)]
struct PeerInfo {
    last_seen: Instant,
    is_leader: bool,
    leader_tcp_base: Option<String>,
}

/// Tracks live peers discovered via mDNS and determines whether this node should be leader.
///
/// Leader election rule: the node with the lexicographically smallest ID among all live peers
/// (including itself) is the leader. This is a deterministic, coordination-free variant of
/// the bully algorithm that works well over mDNS-based local-network discovery.
pub struct ElectionState {
    pub my_id: String,
    peers: Arc<RwLock<HashMap<String, PeerInfo>>>,
    i_am_leader: Arc<RwLock<bool>>,
}

impl ElectionState {
    pub fn new(my_id: String) -> Self {
        Self {
            my_id,
            peers: Arc::new(RwLock::new(HashMap::new())),
            i_am_leader: Arc::new(RwLock::new(false)),
        }
    }

    pub fn observe_peer(&self, node_id: &str) {
        if node_id == self.my_id {
            return;
        }
        let mut lock = self.peers.write().unwrap();
        let entry = lock.entry(node_id.to_string()).or_insert(PeerInfo {
            last_seen: Instant::now(),
            is_leader: false,
            leader_tcp_base: None,
        });
        entry.last_seen = Instant::now();
    }

    pub fn observe_leader(&self, node_id: &str, leader_tcp_base: &str) {
        if node_id == self.my_id {
            return;
        }
        let mut lock = self.peers.write().unwrap();
        let entry = lock.entry(node_id.to_string()).or_insert(PeerInfo {
            last_seen: Instant::now(),
            is_leader: false,
            leader_tcp_base: None,
        });
        entry.last_seen = Instant::now();
        entry.is_leader = true;
        entry.leader_tcp_base = Some(leader_tcp_base.to_string());
    }

    /// Clear all leader flags (e.g. after the leader goes offline).
    pub fn clear_leader_flags(&self) {
        let mut lock = self.peers.write().unwrap();
        for peer in lock.values_mut() {
            peer.is_leader = false;
        }
    }

    /// Returns `(leader_id, leader_tcp_base)` if a live leader is currently known via mDNS.
    pub fn current_leader(&self) -> Option<(String, String)> {
        let lock = self.peers.read().unwrap();
        let now = Instant::now();
        lock.iter()
            .filter(|(_, p)| p.is_leader && now.duration_since(p.last_seen) < PEER_TIMEOUT)
            .filter_map(|(id, p)| p.leader_tcp_base.as_ref().map(|a| (id.clone(), a.clone())))
            .next()
    }

    /// True when this node has the smallest ID among all live peers (including itself).
    pub fn i_should_be_leader(&self) -> bool {
        let lock = self.peers.read().unwrap();
        let now = Instant::now();
        lock.iter()
            .filter(|(_, p)| now.duration_since(p.last_seen) < PEER_TIMEOUT)
            .all(|(peer_id, _)| self.my_id <= *peer_id)
    }

    pub fn set_i_am_leader(&self, is_leader: bool) {
        *self.i_am_leader.write().unwrap() = is_leader;
    }

    pub fn am_i_leader(&self) -> bool {
        *self.i_am_leader.read().unwrap()
    }

    pub fn live_peer_count(&self) -> usize {
        let lock = self.peers.read().unwrap();
        let now = Instant::now();
        lock.values()
            .filter(|p| now.duration_since(p.last_seen) < PEER_TIMEOUT)
            .count()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn i_am_only_node_so_should_lead() {
        let state = ElectionState::new("node-b".to_string());
        assert!(state.i_should_be_leader());
    }

    #[test]
    fn smaller_id_peer_means_i_should_not_lead() {
        let state = ElectionState::new("node-b".to_string());
        state.observe_peer("node-a");
        assert!(!state.i_should_be_leader());
    }

    #[test]
    fn larger_id_peers_do_not_displace_me() {
        let state = ElectionState::new("node-a".to_string());
        state.observe_peer("node-b");
        state.observe_peer("node-c");
        assert!(state.i_should_be_leader());
    }

    #[test]
    fn current_leader_returns_live_leader() {
        let state = ElectionState::new("node-b".to_string());
        state.observe_leader("node-a", "192.168.1.10");
        let leader = state.current_leader();
        assert!(leader.is_some());
        let (id, addr) = leader.unwrap();
        assert_eq!(id, "node-a");
        assert_eq!(addr, "192.168.1.10");
    }

    #[test]
    fn clear_leader_flags_removes_leader() {
        let state = ElectionState::new("node-b".to_string());
        state.observe_leader("node-a", "192.168.1.10");
        state.clear_leader_flags();
        assert!(state.current_leader().is_none());
    }

    #[test]
    fn self_observe_is_ignored() {
        let state = ElectionState::new("node-a".to_string());
        state.observe_peer("node-a");
        assert_eq!(state.live_peer_count(), 0);
    }
}
