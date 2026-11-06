use std::time::Instant;

use sysinfo::{Disks, ProcessesToUpdate};

use crate::commands::apps::terminate_processes;
use crate::models::{BatteryInfo, DiskInfo, NetworkInfo, ProcessInfo, SystemSnapshot};
use crate::state::AppState;

#[tauri::command(async)]
pub fn system_snapshot(state: tauri::State<AppState>) -> Result<SystemSnapshot, String> {
    let mut sys = state.system.lock().map_err(|e| e.to_string())?;
    sys.refresh_cpu_usage();
    sys.refresh_memory();
    sys.refresh_processes(ProcessesToUpdate::All, true);

    let cpu_usage = sys.global_cpu_usage();
    let cpu_core_usage: Vec<f32> = sys.cpus().iter().map(|cpu| cpu.cpu_usage()).collect();
    let load = sysinfo::System::load_average();

    let memory_total = sys.total_memory();
    let memory_used = sys.used_memory();
    let memory_percent = if memory_total > 0 {
        (memory_used as f64 / memory_total as f64 * 100.0) as f32
    } else {
        0.0
    };

    let mut processes = collect_processes(&sys);
    processes.truncate(12);

    let uptime = sysinfo::System::uptime();
    let swap_total = sys.total_swap();
    let swap_used = sys.used_swap();
    drop(sys);

    let disks = disk_list();
    let network = network_rates(&state)?;
    let battery = battery_cached(&state);

    let health_score = health_score(cpu_usage, memory_percent, &disks, battery.as_ref());

    Ok(SystemSnapshot {
        host: sysinfo::System::host_name().unwrap_or_else(|| "Computer".into()),
        platform: if cfg!(target_os = "macos") {
            "macos".into()
        } else if cfg!(windows) {
            "windows".into()
        } else {
            "linux".into()
        },
        os_name: sysinfo::System::name().unwrap_or_default(),
        os_version: sysinfo::System::os_version().unwrap_or_default(),
        arch: sysinfo::System::cpu_arch(),
        uptime,
        health_score,
        cpu_usage,
        cpu_cores: cpu_core_usage.len(),
        cpu_core_usage,
        load_avg: [load.one, load.five, load.fifteen],
        memory_total,
        memory_used,
        memory_percent,
        swap_total,
        swap_used,
        disks,
        network,
        battery,
        processes,
    })
}

fn collect_processes(sys: &sysinfo::System) -> Vec<ProcessInfo> {
    let current = std::process::id();
    let mut processes: Vec<ProcessInfo> = sys
        .processes()
        .iter()
        .map(|(pid, process)| ProcessInfo {
            pid: pid.as_u32(),
            name: process.name().to_string_lossy().to_string(),
            cpu: process.cpu_usage(),
            memory: process.memory(),
            exe: process
                .exe()
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or_default(),
            is_self: pid.as_u32() == current,
            received: 0,
            transmitted: 0,
        })
        .collect();
    processes.sort_by(|a, b| {
        b.cpu
            .partial_cmp(&a.cpu)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| b.memory.cmp(&a.memory))
            .then_with(|| a.name.cmp(&b.name))
    });
    processes
}

/// 完整进程列表，默认按 CPU 占用从高到低排序。
#[tauri::command(async)]
pub fn list_processes(state: tauri::State<AppState>) -> Result<Vec<ProcessInfo>, String> {
    let mut sys = state.system.lock().map_err(|e| e.to_string())?;
    sys.refresh_processes(ProcessesToUpdate::All, true);
    let mut processes = collect_processes(&sys);
    drop(sys);
    attach_network_rates(&state, &mut processes);
    Ok(processes)
}

/// 读取每个进程的累计网络字节数，键为 PID。
#[cfg(target_os = "macos")]
fn process_network_totals() -> Option<std::collections::HashMap<u32, (u64, u64)>> {
    // -L 1 只输出一次 CSV 采样，开销远小于交互模式。
    let output = crate::util::command("/usr/bin/nettop")
        .args(["-P", "-x", "-L", "1", "-J", "bytes_in,bytes_out"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }

    let text = String::from_utf8_lossy(&output.stdout);
    let mut totals = std::collections::HashMap::new();
    for line in text.lines().skip(1) {
        // 形如 `mDNSResponder.239,123,456,`，进程名里可能有小数点，从右侧切分 PID。
        let mut parts = line.split(',');
        let Some(label) = parts.next() else { continue };
        let Some((_, pid)) = label.rsplit_once('.') else { continue };
        let Ok(pid) = pid.parse::<u32>() else { continue };
        let received = parts.next().and_then(|v| v.parse::<u64>().ok()).unwrap_or(0);
        let transmitted = parts.next().and_then(|v| v.parse::<u64>().ok()).unwrap_or(0);
        totals.insert(pid, (received, transmitted));
    }
    Some(totals)
}

#[cfg(not(target_os = "macos"))]
fn process_network_totals() -> Option<std::collections::HashMap<u32, (u64, u64)>> {
    None
}

/// 用两次累计值之间的差值换算出每个进程当前的上下行速率。
fn attach_network_rates(state: &AppState, processes: &mut [ProcessInfo]) {
    let Some(totals) = process_network_totals() else {
        return;
    };
    let now = Instant::now();
    let Ok(mut baseline) = state.net_process_baseline.lock() else {
        return;
    };

    if let Some((at, previous)) = baseline.as_ref() {
        let elapsed = now.duration_since(*at).as_secs_f64();
        // 间隔过短会放大采样误差，过长则说明列表已关闭很久，本次只重建基准。
        if (0.2..=15.0).contains(&elapsed) {
            for process in processes.iter_mut() {
                let Some((received, transmitted)) = totals.get(&process.pid) else {
                    continue;
                };
                let (prev_received, prev_transmitted) = previous
                    .get(&process.pid)
                    .copied()
                    .unwrap_or((0, 0));
                process.received =
                    (received.saturating_sub(prev_received) as f64 / elapsed) as u64;
                process.transmitted =
                    (transmitted.saturating_sub(prev_transmitted) as f64 / elapsed) as u64;
            }
        }
    }

    *baseline = Some((now, totals));
}

/// 先请求进程正常退出，超时后强制结束。
#[tauri::command(async)]
pub fn kill_process(pid: u32) -> Result<(), String> {
    if pid <= 1 {
        return Err("系统关键进程不能结束".into());
    }
    if pid == std::process::id() {
        return Err("不能结束 Mole Desktop 自身".into());
    }

    let target = sysinfo::Pid::from_u32(pid);
    let mut sys = sysinfo::System::new();
    sys.refresh_processes(ProcessesToUpdate::All, true);
    let Some(process) = sys.process(target) else {
        return Err("进程不存在或已经结束".into());
    };
    let name = process.name().to_string_lossy().to_string();

    let remaining = terminate_processes(&[pid]);
    if remaining.is_empty() {
        Ok(())
    } else {
        Err(format!(
            "无法结束「{name}」（PID {pid}），可能需要管理员权限"
        ))
    }
}

fn disk_list() -> Vec<DiskInfo> {
    let disks = Disks::new_with_refreshed_list();
    let mut out: Vec<DiskInfo> = Vec::new();
    for disk in disks.list() {
        let mount = disk.mount_point().to_string_lossy().to_string();
        if mount.is_empty() || disk.total_space() == 0 {
            continue;
        }
        if mount.starts_with("/System/Volumes/") {
            continue;
        }
        let fs = disk.file_system().to_string_lossy().to_string();
        if fs.contains("devfs") || fs.contains("autofs") {
            continue;
        }
        let total = disk.total_space();
        let free = disk.available_space();
        if out.iter().any(|d| d.total == total && d.free == free) {
            continue;
        }
        let used_percent = if total > 0 {
            ((total - free) as f64 / total as f64 * 100.0) as f32
        } else {
            0.0
        };
        let name = disk.name().to_string_lossy().to_string();
        out.push(DiskInfo {
            name: if name.is_empty() { mount.clone() } else { name },
            mount,
            total,
            free,
            used_percent,
            file_system: fs,
        });
    }
    out
}

fn network_rates(state: &AppState) -> Result<NetworkInfo, String> {
    let mut nets = state.networks.lock().map_err(|e| e.to_string())?;
    nets.refresh(true);
    let mut total_received = 0u64;
    let mut total_transmitted = 0u64;
    for (name, data) in nets.iter() {
        if name == "lo" || name == "lo0" || name.starts_with("Loopback") {
            continue;
        }
        total_received = total_received.saturating_add(data.total_received());
        total_transmitted = total_transmitted.saturating_add(data.total_transmitted());
    }
    drop(nets);

    let now = Instant::now();
    let mut baseline = state.net_baseline.lock().map_err(|e| e.to_string())?;
    let (rx_rate, tx_rate) = match baseline.as_ref() {
        Some((at, rx, tx)) => {
            let elapsed = now.duration_since(*at).as_secs_f64().max(0.2);
            let drx = total_received.saturating_sub(*rx);
            let dtx = total_transmitted.saturating_sub(*tx);
            ((drx as f64 / elapsed) as u64, (dtx as f64 / elapsed) as u64)
        }
        None => (0, 0),
    };
    *baseline = Some((now, total_received, total_transmitted));

    Ok(NetworkInfo {
        received_per_sec: rx_rate,
        transmitted_per_sec: tx_rate,
        total_received,
        total_transmitted,
    })
}

fn battery_cached(state: &AppState) -> Option<BatteryInfo> {
    let mut cache = state.battery.lock().ok()?;
    if let Some((at, value)) = cache.as_ref() {
        if at.elapsed().as_secs() < 60 {
            return value.clone();
        }
    }
    let value = read_battery();
    *cache = Some((Instant::now(), value.clone()));
    value
}

fn read_battery() -> Option<BatteryInfo> {
    #[cfg(target_os = "macos")]
    {
        let output = crate::util::command("pmset")
            .arg("-g")
            .arg("batt")
            .output()
            .ok()?;
        let text = String::from_utf8_lossy(&output.stdout);
        for line in text.lines() {
            if !line.contains('%') {
                continue;
            }
            let percent = line
                .split_whitespace()
                .find_map(|part| part.trim_end_matches(';').strip_suffix('%'))
                .and_then(|value| value.parse::<f32>().ok())?;
            let status = if line.contains("discharging") {
                "使用电池"
            } else if line.contains("charging") {
                "充电中"
            } else if line.contains("charged") {
                "已充满"
            } else {
                "已接通电源"
            };
            return Some(BatteryInfo {
                percent,
                status: status.to_string(),
            });
        }
        None
    }
    #[cfg(windows)]
    {
        let output = crate::util::command("powershell")
            .args([
                "-NoProfile",
                "-Command",
                "Get-CimInstance Win32_Battery | Select-Object -First 1 EstimatedChargeRemaining,BatteryStatus | ConvertTo-Json -Compress",
            ])
            .output()
            .ok()?;
        let text = String::from_utf8_lossy(&output.stdout);
        let value: serde_json::Value = serde_json::from_str(text.trim()).ok()?;
        let percent = value.get("EstimatedChargeRemaining")?.as_f64()? as f32;
        let status_code = value
            .get("BatteryStatus")
            .and_then(|v| v.as_i64())
            .unwrap_or(0);
        let status = match status_code {
            1 => "使用电池",
            2 => "已接通电源",
            6 | 7 | 8 | 9 => "充电中",
            _ => "未知",
        };
        Some(BatteryInfo {
            percent,
            status: status.to_string(),
        })
    }
    #[cfg(not(any(target_os = "macos", windows)))]
    {
        None
    }
}

fn health_score(cpu: f32, memory_percent: f32, disks: &[DiskInfo], battery: Option<&BatteryInfo>) -> u8 {
    let mut score = 100.0f32;
    if cpu > 50.0 {
        score -= ((cpu - 50.0) * 0.5).min(25.0);
    }
    if memory_percent > 70.0 {
        score -= ((memory_percent - 70.0) * 0.4).min(12.0);
    }
    if let Some(disk) = disks.first() {
        let free_percent = 100.0 - disk.used_percent;
        if free_percent < 10.0 {
            score -= 20.0;
        } else if free_percent < 20.0 {
            score -= 10.0;
        }
    }
    if let Some(battery) = battery {
        if battery.percent < 20.0 && battery.status == "使用电池" {
            score -= 5.0;
        }
    }
    score.clamp(30.0, 100.0) as u8
}
