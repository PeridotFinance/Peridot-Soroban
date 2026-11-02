//! Per-call resource evidence from the host's invocation meter.
//!
//! This module never changes budgets, limits, authorization, or contract state.
//! Capture immediately after the top-level invocation (including a caught limit
//! failure), before another invocation replaces the host's last-call tree.
//! CPU is inclusive; exclusive CPU subtracts direct children. Host25 memory is
//! cumulative modeled charges. Host29 memory is growth in a refunded meter's
//! high-water mark: subtracting children attributes peak growth, not allocations
//! or independent per-call peaks. Neither is process peak RSS.
//! Ledger fields are signed net deltas and are not additive footprint sizes.
use soroban_sdk::{Address, Env};
use std::{
    cell::RefCell,
    format,
    io::Write,
    string::{String, ToString},
    vec::Vec,
};

std::thread_local! {
    static CONTRACTS: RefCell<Vec<(String,String,String)>> = const { RefCell::new(Vec::new()) };
}

/// Register human-readable role labels during fixture setup. The debug address
/// is matched against the host's diagnostic invocation descriptor; the StrKey
/// is also retained so the report can be reconciled against the fixture.
pub(super) fn register_contracts(_env: &Env, contracts: &[(&str, &Address)]) {
    let labels = contracts
        .iter()
        .map(|(role, address)| {
            let xdr: soroban_sdk::xdr::ScAddress = (*address).into();
            (
                String::from(*role),
                format!("{xdr:?}"),
                address.to_string().to_string(),
            )
        })
        .collect();
    CONTRACTS.with(|c| *c.borrow_mut() = labels);
}

/// Append a full invocation tree as one JSON line when explicitly requested.
/// `run_mode` must distinguish enforced acceptance/diagnostics from profiling.
pub(super) fn capture(env: &Env, flow: &str, ok: bool, profile: &str, run_mode: &str) {
    let Ok(output) = std::env::var("CL_CALL_PROFILE_REPORT") else {
        return;
    };
    let tree = env
        .host()
        .get_detailed_last_invocation_resources()
        .expect("call profiling requested but detailed invocation metering unavailable");
    let root_cpu = tree.resources.instructions;
    let root_memory = tree.resources.mem_bytes;
    let runtime_protocol = soroban_env_host::meta::INTERFACE_VERSION.protocol;
    let memory_semantics = match runtime_protocol {
        25 => "cumulative_modeled_memory_charges",
        29 => "global_high_water_mark_growth_with_refunds",
        _ => panic!("memory profiling semantics unreviewed for host protocol {runtime_protocol}"),
    };
    let mut stack = std::vec![(tree, String::from("0"), 0u32)];
    let mut calls = Vec::new();
    let mut descriptors_complete = true;
    while let Some((node, path, depth)) = stack.pop() {
        let invocation = format!("{:?}", node.invocation);
        let (role, address) = CONTRACTS.with(|c| {
            c.borrow()
                .iter()
                .find(|(_, needle, _)| invocation.contains(needle))
                .map(|(role, _, address)| (role.clone(), Some(address.clone())))
                .unwrap_or((String::from("unlabelled"), None))
        });
        // The descriptor is retained verbatim. The function convenience label
        // is not used for metering or limit decisions.
        let function = invocation
            .rsplit_once("ScSymbol(StringM(")
            .and_then(|(_, tail)| tail.strip_suffix(")))"))
            .map(String::from)
            .unwrap_or_else(|| invocation.clone());
        // Host debug-mode descriptor conversion uses default address/symbol
        // after a shadow-budget failure. Such a tree cannot support reliable
        // per-function profiling, even if numeric totals still reconcile.
        if function.is_empty() {
            descriptors_complete = false;
        }
        let child_cpu: i64 = node
            .sub_call_resources
            .iter()
            .map(|c| c.resources.instructions)
            .sum();
        let child_memory: i64 = node
            .sub_call_resources
            .iter()
            .map(|c| c.resources.mem_bytes)
            .sum();
        let r = &node.resources;
        calls.push(serde_json::json!({
            "path":path,"depth":depth,"role":role,"address":address,
            "function":function,"invocation":invocation,
            "inclusive":{"instructions":r.instructions,"memory_bytes":r.mem_bytes,
                "disk_read_entries_delta":r.disk_read_entries,
                "memory_read_entries_delta":r.memory_read_entries,
                "write_entries_delta":r.write_entries,"disk_read_bytes_delta":r.disk_read_bytes,
                "write_bytes_delta":r.write_bytes,"event_bytes_delta":r.contract_events_size_bytes,
                "persistent_rent_ledger_bytes_delta":r.persistent_rent_ledger_bytes,
                "persistent_entry_rent_bumps_delta":r.persistent_entry_rent_bumps,
                "temporary_rent_ledger_bytes_delta":r.temporary_rent_ledger_bytes,
                "temporary_entry_rent_bumps_delta":r.temporary_entry_rent_bumps},
            "exclusive":{"instructions":r.instructions-child_cpu,
                "memory_counter_contribution_bytes":r.mem_bytes-child_memory},
            "direct_child_count":node.sub_call_resources.len()
        }));
        for (index, child) in node.sub_call_resources.into_iter().enumerate().rev() {
            stack.push((child, format!("{path}.{index}"), depth + 1));
        }
    }
    let exclusive_cpu: i64 = calls
        .iter()
        .map(|c| c["exclusive"]["instructions"].as_i64().unwrap())
        .sum();
    let exclusive_memory: i64 = calls
        .iter()
        .map(|c| {
            c["exclusive"]["memory_counter_contribution_bytes"]
                .as_i64()
                .unwrap()
        })
        .sum();
    assert_eq!(
        exclusive_cpu, root_cpu,
        "call CPU breakdown does not reconcile"
    );
    assert_eq!(
        exclusive_memory, root_memory,
        "call memory breakdown does not reconcile"
    );
    // Cost-type trackers identify allocation/serialization/VM work across the
    // budget interval. Keep these distinct from the invocation tree: SDK work
    // outside the invocation can remain here, and refunds/peaks differ on29.
    let budget = env.cost_estimate().budget();
    let cost_types: Vec<_> = soroban_sdk::xdr::ContractCostType::variants()
        .iter()
        .filter_map(|kind| {
            let t = budget.tracker(*kind);
            if t.iterations == 0 {
                return None;
            }
            Some(
                serde_json::json!({"cost_type":format!("{kind:?}"),"iterations":t.iterations,
            "inputs":t.inputs,"instructions":t.cpu,"memory_bytes":t.mem}),
            )
        })
        .collect();
    let host_budget = env.host().budget_cloned();
    let shadow = serde_json::json!({
        "instructions":host_budget.get_shadow_cpu_insns_consumed().unwrap(),
        "memory_bytes":host_budget.get_shadow_mem_bytes_consumed().unwrap(),
        "cpu_limit_exceeded":host_budget.shadow_cpu_limit_exceeded().unwrap(),
        "memory_limit_exceeded":host_budget.shadow_mem_limit_exceeded().unwrap(),
        "semantics":"optional SDK diagnostics/auth/storage snapshots only; excluded from execution ceilings"
    });
    let report = serde_json::json!({
        "schema":"cl-call-profile-v2","flow":flow,"invocation_ok":ok,
        "profile":profile,"run_mode":run_mode,
        "runtime_protocol":runtime_protocol,"memory_semantics":memory_semantics,
        "measurement":"host modeled CPU and memory counter; not peak RSS or live-state proof",
        "memory_attribution":"parent counter delta minus direct child deltas; on host29 this attributes new global high-water growth, not self allocations or independent call peaks",
        "cost_tracker_semantics":"budget-interval cumulative cost charges; may include post-invocation SDK work; not reconciled to invocation tree or refunded peak memory",
        "ledger_semantics":"signed net changes; do not sum as footprint cardinality",
        "root_instructions":root_cpu,"root_memory_bytes":root_memory,
        "exclusive_reconciles":true,"diagnostic_descriptors_complete":descriptors_complete,
        "calls":calls,"cost_type_trackers":cost_types,"shadow_instrumentation":shadow
    });
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(output)
        .unwrap();
    writeln!(file, "{report}").unwrap();
}
