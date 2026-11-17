//! Which processes belong to a watched tree.
//!
//! Membership follows observed parentage only: a process joins when a scan
//! shows its parent is already a member. Once in, it stays until it exits,
//! so a child that is later re-parented to launchd is still tracked.
//! Identity is PID plus start time, so a reused PID is a different process.
//!
//! This is observation, not containment. A process whose parent exits
//! between two scans is never seen as that parent's child and is missed.

use std::collections::{BTreeMap, HashMap};

use crate::procinfo::{ProcIdentity, ProcStart};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Change {
    /// A root named by the caller, reported on the first scan.
    Root(ProcIdentity),
    /// A process first seen with a tracked parent.
    Joined(ProcIdentity),
    /// A tracked process that is gone, or whose PID now names another process.
    Exited(ProcIdentity),
}

#[derive(Clone, Copy)]
struct Member {
    identity: ProcIdentity,
    announced: bool,
}

pub struct Tracker {
    // Ordered so every scan reports changes in the same order.
    members: BTreeMap<u32, Member>,
}

impl Tracker {
    /// Roots are members from the start, whatever the first scan shows.
    pub fn new(roots: &[ProcIdentity]) -> Self {
        let members = roots
            .iter()
            .map(|root| {
                let member = Member {
                    identity: *root,
                    announced: false,
                };
                (root.pid, member)
            })
            .collect();
        Self { members }
    }

    /// Apply one process-table scan. Order: roots not yet reported, then
    /// joins (parents before their children), then exits.
    pub fn update(&mut self, table: &[ProcIdentity]) -> Vec<Change> {
        let mut changes = Vec::new();
        for member in self.members.values_mut().filter(|m| !m.announced) {
            member.announced = true;
            changes.push(Change::Root(member.identity));
        }

        let live: HashMap<u32, ProcStart> = table.iter().map(|p| (p.pid, p.start)).collect();
        let gone: Vec<ProcIdentity> = self
            .members
            .values()
            .filter(|m| live.get(&m.identity.pid) != Some(&m.identity.start))
            .map(|m| m.identity)
            .collect();
        for identity in &gone {
            self.members.remove(&identity.pid);
        }

        let mut children: BTreeMap<u32, Vec<ProcIdentity>> = BTreeMap::new();
        for process in table {
            children.entry(process.ppid).or_default().push(*process);
        }
        for list in children.values_mut() {
            list.sort_by_key(|p| p.pid);
        }
        let mut queue: Vec<u32> = self.members.keys().rev().copied().collect();
        while let Some(parent) = queue.pop() {
            let Some(list) = children.get(&parent) else {
                continue;
            };
            let mut joined = Vec::new();
            for child in list {
                // Start times are wall-clock, so a clock stepped backwards can
                // make a real child look older than its parent. Parentage is
                // not checked against them: a missed descendant is the worse
                // error for a monitor than an extra one.
                if child.pid == parent || self.members.contains_key(&child.pid) {
                    continue;
                }
                let member = Member {
                    identity: *child,
                    announced: true,
                };
                self.members.insert(child.pid, member);
                changes.push(Change::Joined(*child));
                joined.push(child.pid);
            }
            // Depth first, lowest PID next.
            queue.extend(joined.into_iter().rev());
        }

        changes.extend(gone.into_iter().map(Change::Exited));
        changes
    }

    /// Start time of the tracked process with this PID, if any.
    pub fn start_of(&self, pid: u32) -> Option<ProcStart> {
        self.members.get(&pid).map(|m| m.identity.start)
    }

    /// Member PIDs in ascending order.
    pub fn pids(&self) -> impl Iterator<Item = u32> + '_ {
        self.members.keys().copied()
    }

    pub fn contains(&self, pid: u32) -> bool {
        self.members.contains_key(&pid)
    }

    pub fn is_empty(&self) -> bool {
        self.members.is_empty()
    }

    pub fn len(&self) -> usize {
        self.members.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(pid: u32, ppid: u32, sec: u64) -> ProcIdentity {
        ProcIdentity {
            pid,
            ppid,
            start: ProcStart { sec, usec: 0 },
        }
    }

    #[test]
    fn descendants_join_at_any_depth_and_unrelated_processes_do_not() {
        let root = p(100, 1, 10);
        let mut tree = Tracker::new(&[root]);
        // The grandchild is listed before its parent; order must not matter.
        let table = [
            p(1, 0, 1),
            p(300, 200, 12),
            root,
            p(200, 100, 11),
            p(999, 1, 11),
        ];
        assert_eq!(
            tree.update(&table),
            [
                Change::Root(root),
                Change::Joined(p(200, 100, 11)),
                Change::Joined(p(300, 200, 12)),
            ]
        );
        assert!(tree.contains(100) && tree.contains(200) && tree.contains(300));
        assert!(!tree.contains(999) && !tree.contains(1));
        assert_eq!(tree.len(), 3);
        // Nothing new: a repeat scan is silent.
        assert_eq!(tree.update(&table), []);
    }

    #[test]
    fn reparented_descendant_stays_tracked_after_its_parent_exits() {
        let root = p(100, 1, 10);
        let mut tree = Tracker::new(&[root]);
        tree.update(&[root, p(200, 100, 11)]);
        // The root exits; launchd adopts the child.
        let adopted = p(200, 1, 11);
        assert_eq!(tree.update(&[adopted]), [Change::Exited(root)]);
        assert!(tree.contains(200));
        // The orphan's own later children still join.
        assert_eq!(
            tree.update(&[adopted, p(300, 200, 20)]),
            [Change::Joined(p(300, 200, 20))]
        );
        assert_eq!(
            tree.update(&[]),
            [
                Change::Exited(p(200, 100, 11)),
                Change::Exited(p(300, 200, 20))
            ]
        );
        assert!(tree.is_empty());
    }

    #[test]
    fn reused_pid_is_a_different_process() {
        let root = p(100, 1, 10);
        let child = p(200, 100, 11);
        let mut tree = Tracker::new(&[root]);
        tree.update(&[root, child]);
        // PID 200 now belongs to an unrelated process started later.
        let stranger = p(200, 1, 50);
        assert_eq!(tree.update(&[root, stranger]), [Change::Exited(child)]);
        assert!(!tree.contains(200));
        assert_eq!(tree.start_of(100), Some(root.start));
        // A reused root PID ends the root too.
        assert_eq!(tree.update(&[p(100, 1, 60)]), [Change::Exited(root)]);
        assert!(tree.is_empty());
    }

    #[test]
    fn parentage_does_not_depend_on_start_time_order() {
        // The wall clock stepped back between the two starts.
        let root = p(100, 1, 50);
        let child = p(200, 100, 11);
        let mut tree = Tracker::new(&[root]);
        assert_eq!(
            tree.update(&[root, child]),
            [Change::Root(root), Change::Joined(child)]
        );
        // PID 0 is its own parent in the kernel table; it never joins itself.
        let kernel = p(0, 0, 1);
        let mut tree = Tracker::new(&[kernel]);
        assert_eq!(tree.update(&[kernel]), [Change::Root(kernel)]);
        assert_eq!(tree.len(), 1);
    }

    #[test]
    fn several_roots_are_tracked_together_and_reported_once() {
        let (a, b) = (p(100, 1, 10), p(500, 1, 10));
        let mut tree = Tracker::new(&[b, a, a]);
        assert_eq!(
            tree.update(&[a, b, p(101, 100, 11), p(501, 500, 11)]),
            [
                Change::Root(a),
                Change::Root(b),
                Change::Joined(p(101, 100, 11)),
                Change::Joined(p(501, 500, 11)),
            ]
        );
        // One root exits; the other tree is unaffected.
        assert_eq!(
            tree.update(&[b, p(501, 500, 11)]),
            [Change::Exited(a), Change::Exited(p(101, 100, 11))]
        );
        assert_eq!(tree.len(), 2);
    }

    #[test]
    fn a_root_that_is_already_gone_is_reported_then_exits() {
        let root = p(100, 1, 10);
        let mut tree = Tracker::new(&[root]);
        assert_eq!(tree.update(&[]), [Change::Root(root), Change::Exited(root)]);
        assert!(tree.is_empty());
        assert_eq!(tree.update(&[root]), []);
    }
}
