use tauri::State;

use crate::app::AppState;
use crate::error::AppResult;
use crate::infra::db::repos::item_meta_repo::ItemMetaRepo;
use crate::infra::db::repos::tags_repo::{Tag, TagsRepo};

#[tauri::command]
#[specta::specta]
pub async fn set_favorite(
    state: State<'_, AppState>,
    asset_id: String,
    favorite: bool,
) -> AppResult<()> {
    ItemMetaRepo::set_favorite(&state.db, &asset_id, favorite).await
}

#[tauri::command]
#[specta::specta]
pub async fn set_pinned(
    state: State<'_, AppState>,
    asset_id: String,
    pinned: bool,
) -> AppResult<()> {
    ItemMetaRepo::set_pinned(&state.db, &asset_id, pinned).await
}

#[tauri::command]
#[specta::specta]
pub async fn set_folder(
    state: State<'_, AppState>,
    asset_id: String,
    folder: Option<String>,
) -> AppResult<()> {
    ItemMetaRepo::set_folder(&state.db, &asset_id, folder.as_deref()).await
}

#[tauri::command]
#[specta::specta]
pub async fn set_note(
    state: State<'_, AppState>,
    asset_id: String,
    note: Option<String>,
) -> AppResult<()> {
    ItemMetaRepo::set_note(&state.db, &asset_id, note.as_deref()).await
}

#[tauri::command]
#[specta::specta]
pub async fn set_custom_label(
    state: State<'_, AppState>,
    asset_id: String,
    label: Option<String>,
) -> AppResult<()> {
    ItemMetaRepo::set_custom_label(&state.db, &asset_id, label.as_deref()).await
}

#[tauri::command]
#[specta::specta]
pub async fn list_tags(state: State<'_, AppState>) -> AppResult<Vec<Tag>> {
    TagsRepo::list(&state.db).await
}

#[tauri::command]
#[specta::specta]
pub async fn create_tag(state: State<'_, AppState>, name: String, color: String) -> AppResult<i32> {
    TagsRepo::create(&state.db, &name, &color).await
}

#[tauri::command]
#[specta::specta]
pub async fn delete_tag(state: State<'_, AppState>, tag_id: i32) -> AppResult<()> {
    TagsRepo::delete(&state.db, tag_id).await
}

#[tauri::command]
#[specta::specta]
pub async fn add_item_tag(
    state: State<'_, AppState>,
    asset_id: String,
    tag_id: i32,
) -> AppResult<()> {
    TagsRepo::add_to_item(&state.db, &asset_id, tag_id).await
}

#[tauri::command]
#[specta::specta]
pub async fn remove_item_tag(
    state: State<'_, AppState>,
    asset_id: String,
    tag_id: i32,
) -> AppResult<()> {
    TagsRepo::remove_from_item(&state.db, &asset_id, tag_id).await
}
