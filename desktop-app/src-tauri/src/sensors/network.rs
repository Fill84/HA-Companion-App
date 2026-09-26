use serde::{Deserialize, Serialize};
#[cfg(windows)]
use std::collections::HashMap;
use sysinfo::Networks;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkData {
    pub interfaces: Vec<NetworkInterface>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkInterface {
    pub name: String,
    pub mac_address: String,
    pub physical_id: Option<String>,
    pub received_bytes: u64,
    pub transmitted_bytes: u64,
    pub ip_addresses: Vec<String>,
}

fn mac_identity(address: &str) -> Option<String> {
    let octets: Vec<_> = address.split(':').collect();
    (octets.len() == 6
        && octets
            .iter()
            .all(|part| part.len() == 2 && part.chars().all(|c| c.is_ascii_hexdigit()))
        && !octets.iter().all(|part| *part == "00")
        && !octets.iter().all(|part| part.eq_ignore_ascii_case("ff")))
    .then(|| format!("mac:{}", address.to_ascii_lowercase()))
}

#[cfg(windows)]
fn windows_adapter_ids() -> HashMap<String, Option<String>> {
    use windows::Win32::NetworkManagement::IpHelper::{
        GetAdaptersAddresses, GET_ADAPTERS_ADDRESSES_FLAGS, IP_ADAPTER_ADDRESSES_LH,
    };

    const ERROR_BUFFER_OVERFLOW: u32 = 111;
    const MAX_BYTES: u32 = 1_048_576;
    // Skip address lists; only the friendly name and persistent adapter GUID are needed.
    let flags = GET_ADAPTERS_ADDRESSES_FLAGS(1 | 2 | 4 | 8 | 256);
    let mut size = 16_384u32;
    let mut result = HashMap::new();
    for _ in 0..2 {
        let mut buffer = vec![0u64; (size as usize).div_ceil(8)];
        let code = unsafe {
            GetAdaptersAddresses(
                0,
                flags,
                None,
                Some(buffer.as_mut_ptr().cast::<IP_ADAPTER_ADDRESSES_LH>()),
                &mut size,
            )
        };
        if code == ERROR_BUFFER_OVERFLOW && size <= MAX_BYTES {
            continue;
        }
        if code != 0 {
            return result;
        }
        let mut current = buffer.as_ptr().cast::<IP_ADAPTER_ADDRESSES_LH>();
        while !current.is_null() {
            let adapter = unsafe { &*current };
            if let (Ok(name), Ok(raw_id)) = (unsafe { adapter.FriendlyName.to_string() }, unsafe {
                adapter.AdapterName.to_string()
            }) {
                let id = uuid::Uuid::parse_str(raw_id.trim_matches(['{', '}']))
                    .ok()
                    .map(|id| format!("guid:{id}"));
                let key = name.to_lowercase();
                if !name.is_empty() && id.is_some() {
                    // Duplicate friendly names cannot safely identify either interface.
                    result
                        .entry(key)
                        .and_modify(|known| *known = None)
                        .or_insert(id);
                }
            }
            current = adapter.Next;
        }
        return result;
    }
    result
}

pub struct NetworkCollector {
    networks: Networks,
    discovered: bool,
    #[cfg(windows)]
    adapter_ids: HashMap<String, Option<String>>,
}

impl Default for NetworkCollector {
    fn default() -> Self {
        Self {
            networks: Networks::new(),
            discovered: false,
            #[cfg(windows)]
            adapter_ids: HashMap::new(),
        }
    }
}

impl NetworkCollector {
    pub fn collect(&mut self, refresh_topology: bool) -> NetworkData {
        if refresh_topology || !self.discovered {
            self.networks.refresh_list();
            #[cfg(windows)]
            {
                self.adapter_ids = windows_adapter_ids();
            }
            self.discovered = true;
        } else {
            self.networks.refresh();
        }
        self.snapshot()
    }

    fn snapshot(&self) -> NetworkData {
        let interfaces: Vec<NetworkInterface> = self
            .networks
            .iter()
            .map(|(name, data)| {
                let mac_address = data.mac_address().to_string();
                NetworkInterface {
                    name: name.clone(),
                    #[cfg(windows)]
                    physical_id: self
                        .adapter_ids
                        .get(&name.to_lowercase())
                        .cloned()
                        .unwrap_or_else(|| mac_identity(&mac_address)),
                    #[cfg(not(windows))]
                    physical_id: mac_identity(&mac_address),
                    mac_address,
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
}

#[cfg(all(test, windows))]
fn collect() -> NetworkData {
    NetworkCollector::default().collect(true)
}

#[cfg(test)]
mod tests {
    use super::mac_identity;

    #[test]
    fn only_nonempty_hardware_addresses_can_identify_an_interface() {
        assert_eq!(
            mac_identity("AA:BB:CC:00:11:22").as_deref(),
            Some("mac:aa:bb:cc:00:11:22")
        );
        assert_eq!(mac_identity("00:00:00:00:00:00"), None);
        assert_eq!(mac_identity("ff:ff:ff:ff:ff:ff"), None);
        assert_eq!(mac_identity("invalid"), None);
    }

    #[cfg(windows)]
    #[test]
    #[ignore = "requires a Windows host with at least one network adapter"]
    fn windows_adapter_api_exposes_persistent_guids() {
        let ids = super::windows_adapter_ids();
        assert!(ids.values().flatten().any(|id| id.starts_with("guid:")));
        assert!(super::collect().interfaces.iter().any(|interface| interface
            .physical_id
            .as_deref()
            .is_some_and(|id| id.starts_with("guid:"))));
    }
}
