fn main() {
    tauri_build::try_build(
        tauri_build::Attributes::new().app_manifest(
            tauri_build::AppManifest::new().commands(&[
                "get_settings",
                "get_integration_version",
                "save_settings",
                "register_device",
                "reregister_device",
                "check_connection",
                "get_sensor_list",
                "update_sensors_now",
                "toggle_sensor",
                "get_current_language",
                "get_my_public_ip",
                "load_dashboard",
                "hide_dashboard",
            ]),
        ),
    )
    .expect("Could not build Tauri command permissions");
}
