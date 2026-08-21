fn main() {
    let manifest = tauri_build::AppManifest::new().commands(&[
        "desktop_state",
        "desktop_acknowledge_snapshot",
        "desktop_acknowledge_change",
        "desktop_create_work_item",
        "desktop_complete_work_item",
        "desktop_cancel_work_item",
        "desktop_acknowledge_attention_signal",
        "desktop_create_reminder",
        "desktop_acknowledge_reminder_fire",
        "desktop_snooze_reminder_fire",
    ]);
    if let Err(error) =
        tauri_build::try_build(tauri_build::Attributes::new().app_manifest(manifest))
    {
        panic!("failed to generate restricted desktop ACL: {error}");
    }
}
