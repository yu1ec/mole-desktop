use std::path::PathBuf;
use std::sync::Mutex;
use std::time::Instant;

use sysinfo::{Networks, System};

use crate::models::BatteryInfo;

pub struct AppState {
    pub system: Mutex<System>,
    pub networks: Mutex<Networks>,
    pub net_baseline: Mutex<Option<(Instant, u64, u64)>>,
    pub battery: Mutex<Option<(Instant, Option<BatteryInfo>)>>,
    pub data_dir: PathBuf,
}

impl AppState {
    pub fn new(data_dir: PathBuf) -> Self {
        let mut system = System::new();
        system.refresh_cpu_all();
        system.refresh_memory();
        let networks = Networks::new_with_refreshed_list();
        Self {
            system: Mutex::new(system),
            networks: Mutex::new(networks),
            net_baseline: Mutex::new(None),
            battery: Mutex::new(None),
            data_dir,
        }
    }
}
