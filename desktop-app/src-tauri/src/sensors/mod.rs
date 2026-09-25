pub mod battery;
pub mod catalog;
pub mod collector;
pub mod cpu;
pub mod diagnostics;
pub mod disk;
pub mod gpu;
pub mod memory;
pub mod network;
pub mod system_info;
pub mod temperature;

#[cfg(windows)]
pub(crate) fn is_ephemeral_remote_display(pnp_id: Option<&str>, name: &str) -> bool {
    const PREFIX: &str = "SWD\\REMOTEDISPLAYENUM\\";
    pnp_id.is_some_and(|id| {
        id.get(..PREFIX.len())
            .is_some_and(|prefix| prefix.eq_ignore_ascii_case(PREFIX))
    }) || name.eq_ignore_ascii_case("Microsoft Remote Display Adapter")
}

#[cfg(all(test, windows))]
mod remote_display_tests {
    use super::is_ephemeral_remote_display;

    #[test]
    fn only_session_remote_displays_are_filtered() {
        assert!(is_ephemeral_remote_display(
            Some("SWD\\REMOTEDISPLAYENUM\\RDPIDD_INDIRECTDISPLAY&SESSIONID_0002"),
            "Microsoft Remote Display Adapter"
        ));
        assert!(is_ephemeral_remote_display(
            Some("swd\\remotedisplayenum\\sessionid_0003"),
            "Unknown"
        ));
        assert!(is_ephemeral_remote_display(
            None,
            "Microsoft Remote Display Adapter"
        ));
        assert!(!is_ephemeral_remote_display(
            Some("PCI\\VEN_10DE&DEV_1C02"),
            "NVIDIA GeForce GTX 1060 3GB"
        ));
        assert!(!is_ephemeral_remote_display(
            Some("SWD\\DISPLAY\\PERSISTENT_VIRTUAL_MONITOR"),
            "Persistent virtual monitor"
        ));
    }
}
