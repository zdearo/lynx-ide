//! What a session costs: the memory and CPU time of its process tree, read from the process table
//! when asked for. Nothing is sampled in the background.

use std::collections::{HashMap, HashSet};

use sysinfo::{Pid, ProcessRefreshKind, ProcessesToUpdate, System};

use crate::message::SessionUsage;

/// Measures every session at once, from a single look at the process table.
pub fn measure(sessions: &[(String, Option<u32>)]) -> Vec<SessionUsage> {
    let mut system = System::new();
    system.refresh_processes_specifics(
        ProcessesToUpdate::All,
        true,
        ProcessRefreshKind::nothing().with_memory().with_cpu(),
    );
    let mut children: HashMap<Pid, Vec<Pid>> = HashMap::new();
    for (pid, process) in system.processes() {
        if let Some(parent) = process.parent() {
            children.entry(parent).or_default().push(*pid);
        }
    }
    sessions
        .iter()
        .map(|(id, root)| {
            let (memory, cpu) = match root {
                Some(root) => tree_totals(&system, &children, Pid::from_u32(*root)),
                None => (0, 0),
            };
            SessionUsage {
                id: id.clone(),
                memory_kib: memory / 1024,
                cpu_millis: cpu,
            }
        })
        .collect()
}

/// Memory in bytes and CPU time in milliseconds, summed over the root and its descendants.
fn tree_totals(system: &System, children: &HashMap<Pid, Vec<Pid>>, root: Pid) -> (u64, u64) {
    let mut totals = (0, 0);
    let mut seen = HashSet::new();
    let mut pending = vec![root];
    while let Some(pid) = pending.pop() {
        // A pid reused by a new process could point back into the tree; each one counts once.
        if !seen.insert(pid) {
            continue;
        }
        if let Some(process) = system.process(pid) {
            totals.0 += process.memory();
            totals.1 += process.accumulated_cpu_time();
        }
        pending.extend(children.get(&pid).into_iter().flatten().copied());
    }
    totals
}
