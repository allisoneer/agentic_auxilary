use crate::dto::DesktopStateDto;
use crate::error::DesktopErrorDto;
use crate::mutation::AcknowledgeFireInput;
use crate::mutation::AcknowledgeSignalInput;
use crate::mutation::CreateReminderInput;
use crate::mutation::CreateWorkItemInput;
use crate::mutation::ExistingWorkItemInput;
use crate::mutation::MutationReceiptDto;
use crate::mutation::SnoozeFireInput;
use crate::supervisor::DesktopSupervisor;
use tauri::State;

#[tauri::command]
pub async fn desktop_state(
    state: State<'_, DesktopSupervisor>,
) -> Result<DesktopStateDto, DesktopErrorDto> {
    Ok(state.state().await)
}

#[tauri::command]
pub async fn desktop_acknowledge_snapshot(
    state: State<'_, DesktopSupervisor>,
    generation: u64,
    after_cursor: String,
) -> Result<(), DesktopErrorDto> {
    state.acknowledge_snapshot(generation, after_cursor).await
}

#[tauri::command]
pub async fn desktop_acknowledge_change(
    state: State<'_, DesktopSupervisor>,
    generation: u64,
    cursor: String,
) -> Result<(), DesktopErrorDto> {
    state.acknowledge_change(generation, cursor).await
}

#[tauri::command]
pub async fn desktop_create_work_item(
    state: State<'_, DesktopSupervisor>,
    input: CreateWorkItemInput,
) -> Result<MutationReceiptDto, DesktopErrorDto> {
    state.create_work_item(input).await
}
#[tauri::command]
pub async fn desktop_complete_work_item(
    state: State<'_, DesktopSupervisor>,
    input: ExistingWorkItemInput,
) -> Result<MutationReceiptDto, DesktopErrorDto> {
    state.complete_work_item(input).await
}
#[tauri::command]
pub async fn desktop_cancel_work_item(
    state: State<'_, DesktopSupervisor>,
    input: ExistingWorkItemInput,
) -> Result<MutationReceiptDto, DesktopErrorDto> {
    state.cancel_work_item(input).await
}
#[tauri::command]
pub async fn desktop_acknowledge_attention_signal(
    state: State<'_, DesktopSupervisor>,
    input: AcknowledgeSignalInput,
) -> Result<MutationReceiptDto, DesktopErrorDto> {
    state.acknowledge_signal(input).await
}
#[tauri::command]
pub async fn desktop_create_reminder(
    state: State<'_, DesktopSupervisor>,
    input: CreateReminderInput,
) -> Result<MutationReceiptDto, DesktopErrorDto> {
    state.create_reminder(input).await
}
#[tauri::command]
pub async fn desktop_acknowledge_reminder_fire(
    state: State<'_, DesktopSupervisor>,
    input: AcknowledgeFireInput,
) -> Result<MutationReceiptDto, DesktopErrorDto> {
    state.acknowledge_fire(input).await
}
#[tauri::command]
pub async fn desktop_snooze_reminder_fire(
    state: State<'_, DesktopSupervisor>,
    input: SnoozeFireInput,
) -> Result<MutationReceiptDto, DesktopErrorDto> {
    state.snooze_fire(input).await
}
