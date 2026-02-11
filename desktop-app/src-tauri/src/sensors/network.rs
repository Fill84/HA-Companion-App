use serde::{Deserialize, Serialize};
use sysinfo::Networks;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkData {
    pub interfaces: Vec<NetworkInterface>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkInterface {
    pub name: String,
    pub mac_address: String,
    pub received_bytes: u64,
    pub transmitted_bytes: u64,
    pub ip_addresses: Vec<String>,
}

pub fn collect() -> NetworkData {
    let networks = Networks::new_with_refreshed_list();
    let interfaces: Vec<NetworkInterface> = networks
        .iter()
        .map(|(name, data)| {
            NetworkInterface {
                name: name.clone(),
                mac_address: data.mac_address().to_string(),
                received_bytes: data.total_received(),
                transmitted_bytes: data.total_transmitted(),
                ip_addresses: data
                    .ip_networks()
                    .iter()
                    .map(|ip| ip.addr.to_string())
                    .collect(),
            }
        })
        .collect();

    NetworkData { interfaces }
}
