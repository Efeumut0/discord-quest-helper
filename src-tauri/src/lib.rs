use once_cell::sync::OnceCell;
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::env;

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tauri::{AppHandle, Emitter};

mod promo_presence;
mod rpc;
mod runner;
mod updates;

const SEARCH_PRIORITY_TARGET_DEFAULT_COUNT: u32 = 2000;
const SEARCH_PRIORITY_TARGET_MIN_COUNT: u32 = 100;
const SEARCH_PRIORITY_TARGET_MAX_COUNT: u32 = 5000;
const STEAM_TOP_SELLERS_MAX_COUNT: u32 = SEARCH_PRIORITY_TARGET_MAX_COUNT;
const STEAM_SEARCH_PAGE_SIZE: u32 = 100;
const STEAM_TOP_SELLERS_PAGE_DELAY_MS: u64 = 1_100;
const STEAM_TOP_SELLERS_RATE_LIMIT_DELAY_MS: u64 = 5_000;
const STEAM_TOP_SELLERS_MAX_PAGE_RETRIES: u32 = 2;
const STEAM_APPDETAILS_DELAY_MS: u64 = 80;
const EMBEDDED_RUNNER_EXE: &[u8] = include_bytes!("../resources/src-win.exe");
const EMBEDDED_ICON_ICO: &[u8] = include_bytes!("../icons/icon.ico");
const QUEST_PUBLIC_API_URL: &str = "https://api.discordquest.com/api/quests";
const QUEST_RAW_GITHUB_URL: &str =
    "https://raw.githubusercontent.com/aamiaa/discord-api-diff/refs/heads/main/quests.json";
const DISCORD_QUEST_DETAIL_API_BASE: &str = "https://discord.com/api/v10/quests/";
const QUEST_RAW_MAX_EXPIRY_SECONDS: u64 = 180 * 24 * 60 * 60;

#[cfg(target_os = "windows")]
mod process_control {
    use std::collections::HashMap;
    use std::path::{Path, PathBuf};
    use windows::core::PWSTR;
    use windows::Win32::Foundation::CloseHandle;
    use windows::Win32::System::Diagnostics::ToolHelp::{
        CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W,
        TH32CS_SNAPPROCESS,
    };
    use windows::Win32::System::Threading::{
        OpenProcess, QueryFullProcessImageNameW, TerminateProcess, PROCESS_NAME_WIN32,
        PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_TERMINATE,
    };

    fn normalize_path_text(path: &str) -> String {
        path.trim_start_matches(r"\\?\")
            .replace('/', r"\")
            .to_ascii_lowercase()
    }

    fn normalize_target_path(path: &Path) -> String {
        let path_text = std::fs::canonicalize(path)
            .unwrap_or_else(|_| PathBuf::from(path))
            .to_string_lossy()
            .to_string();

        normalize_path_text(&path_text)
    }

    fn with_trailing_separator(path: String) -> String {
        if path.ends_with('\\') {
            path
        } else {
            format!("{path}\\")
        }
    }

    fn query_process_path(process_id: u32) -> Option<String> {
        unsafe {
            let handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, process_id).ok()?;
            let mut buffer = vec![0u16; 32768];
            let mut length = buffer.len() as u32;
            let result = QueryFullProcessImageNameW(
                handle,
                PROCESS_NAME_WIN32,
                PWSTR(buffer.as_mut_ptr()),
                &mut length,
            );
            let _ = CloseHandle(handle);

            result.ok()?;

            let process_path = String::from_utf16_lossy(&buffer[..length as usize]);
            Some(normalize_path_text(&process_path))
        }
    }

    fn snapshot_process_ids() -> Result<Vec<u32>, String> {
        unsafe {
            let snapshot = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0)
                .map_err(|e| format!("Failed to create process snapshot: {}", e))?;
            let mut entry = PROCESSENTRY32W {
                dwSize: std::mem::size_of::<PROCESSENTRY32W>() as u32,
                ..Default::default()
            };
            let mut process_ids = Vec::new();

            if Process32FirstW(snapshot, &mut entry).is_ok() {
                loop {
                    process_ids.push(entry.th32ProcessID);
                    if Process32NextW(snapshot, &mut entry).is_err() {
                        break;
                    }
                }
            }

            let _ = CloseHandle(snapshot);
            Ok(process_ids)
        }
    }

    pub fn running_process_ids_by_path(
        target_paths: &[PathBuf],
    ) -> Result<HashMap<String, Vec<u32>>, String> {
        let mut result: HashMap<String, Vec<u32>> = target_paths
            .iter()
            .map(|path| (normalize_target_path(path), Vec::new()))
            .collect();

        if result.is_empty() {
            return Ok(result);
        }

        for process_id in snapshot_process_ids()? {
            let Some(process_path) = query_process_path(process_id) else {
                continue;
            };

            if let Some(process_ids) = result.get_mut(&process_path) {
                process_ids.push(process_id);
            }
        }

        Ok(result)
    }

    pub fn process_ids_for_paths(target_paths: &[PathBuf]) -> Result<Vec<Vec<u32>>, String> {
        let keys: Vec<String> = target_paths
            .iter()
            .map(|path| normalize_target_path(path))
            .collect();
        let running = running_process_ids_by_path(target_paths)?;

        Ok(keys
            .iter()
            .map(|key| running.get(key).cloned().unwrap_or_default())
            .collect())
    }

    pub fn running_process_paths_under_dir(root_dir: &Path) -> Result<Vec<(u32, String)>, String> {
        let root_dir = with_trailing_separator(normalize_target_path(root_dir));
        let mut matches = Vec::new();

        for process_id in snapshot_process_ids()? {
            let Some(process_path) = query_process_path(process_id) else {
                continue;
            };

            if process_path.starts_with(&root_dir) {
                matches.push((process_id, process_path));
            }
        }

        Ok(matches)
    }

    pub fn is_process_running_at_path(path: &Path) -> Result<bool, String> {
        let path = PathBuf::from(path);
        let running = process_ids_for_paths(&[path])?;
        Ok(running.iter().any(|process_ids| !process_ids.is_empty()))
    }

    pub fn terminate_processes_at_path(path: &Path) -> Result<u32, String> {
        let normalized_path = normalize_target_path(path);
        let running = running_process_ids_by_path(&[PathBuf::from(path)])?;
        let process_ids = running.get(&normalized_path).cloned().unwrap_or_default();
        let mut stopped = 0;

        for process_id in process_ids {
            unsafe {
                let handle = OpenProcess(
                    PROCESS_TERMINATE | PROCESS_QUERY_LIMITED_INFORMATION,
                    false,
                    process_id,
                )
                .map_err(|e| format!("Failed to open process {}: {}", process_id, e))?;

                let terminate_result = TerminateProcess(handle, 0);
                let _ = CloseHandle(handle);

                terminate_result
                    .map_err(|e| format!("Failed to stop process {}: {}", process_id, e))?;
                stopped += 1;
            }
        }

        Ok(stopped)
    }
}

#[cfg(target_os = "windows")]
mod exe_icon {
    use std::ffi::c_void;
    use std::os::windows::ffi::OsStrExt;
    use std::path::Path;

    type Bool = i32;
    type Handle = *mut c_void;
    type PcWstr = *const u16;

    #[link(name = "kernel32")]
    extern "system" {
        fn LoadLibraryExW(file_name: PcWstr, file: Handle, flags: u32) -> Handle;
        fn FreeLibrary(module: Handle) -> Bool;
        fn FindResourceW(module: Handle, name: PcWstr, resource_type: PcWstr) -> Handle;
        fn LoadResource(module: Handle, resource: Handle) -> Handle;
        fn LockResource(resource: Handle) -> *const c_void;
        fn SizeofResource(module: Handle, resource: Handle) -> u32;
        fn BeginUpdateResourceW(file_name: PcWstr, delete_existing_resources: Bool) -> Handle;
        fn UpdateResourceW(
            update: Handle,
            resource_type: PcWstr,
            name: PcWstr,
            language: u16,
            data: *const c_void,
            data_size: u32,
        ) -> Bool;
        fn EndUpdateResourceW(update: Handle, discard: Bool) -> Bool;
    }

    fn wide_path(path: &Path) -> Vec<u16> {
        path.as_os_str().encode_wide().chain(Some(0)).collect()
    }

    fn make_int_resource(id: u16) -> PcWstr {
        id as usize as PcWstr
    }

    fn resource_bytes(path: &Path, name: u16, resource_type: u16) -> Option<Vec<u8>> {
        let file = wide_path(path);
        unsafe {
            // Read resources without executing any code in the target EXE.
            let module = LoadLibraryExW(file.as_ptr(), std::ptr::null_mut(), 0x00000002);
            if module.is_null() {
                return None;
            }
            let resource = FindResourceW(
                module,
                make_int_resource(name),
                make_int_resource(resource_type),
            );
            let bytes = if resource.is_null() {
                None
            } else {
                let size = SizeofResource(module, resource) as usize;
                let loaded = LoadResource(module, resource);
                let data = LockResource(loaded);
                if data.is_null() || size == 0 {
                    None
                } else {
                    Some(std::slice::from_raw_parts(data.cast::<u8>(), size).to_vec())
                }
            };
            FreeLibrary(module);
            bytes
        }
    }

    pub fn icon_image(path: &Path) -> Option<Vec<u8>> {
        resource_bytes(path, 1, 3)
    }

    #[cfg(test)]
    pub fn read_rpc_id(path: &Path) -> Option<Vec<u8>> {
        resource_bytes(path, 102, 10)
    }

    pub fn has_custom_icon(path: &Path, template: &Path) -> bool {
        match (icon_image(path), icon_image(template)) {
            (Some(image), Some(default_image)) => image != default_image,
            _ => false,
        }
    }

    fn build_group_icon(image_size: usize) -> Vec<u8> {
        let mut group = Vec::with_capacity(20);
        group.extend_from_slice(&0u16.to_le_bytes());
        group.extend_from_slice(&1u16.to_le_bytes());
        group.extend_from_slice(&1u16.to_le_bytes());
        group.push(0);
        group.push(0);
        group.push(0);
        group.push(0);
        group.extend_from_slice(&1u16.to_le_bytes());
        group.extend_from_slice(&32u16.to_le_bytes());
        group.extend_from_slice(&(image_size as u32).to_le_bytes());
        group.extend_from_slice(&1u16.to_le_bytes());
        group
    }

    #[allow(clippy::permissions_set_readonly_false)]
    fn begin_update_resource_with_retry(
        raw_path: &Path,
        wide_path: &[u16],
    ) -> Result<Handle, String> {
        if let Ok(mut perms) = std::fs::metadata(raw_path).map(|m| m.permissions()) {
            if perms.readonly() {
                perms.set_readonly(false);
                let _ = std::fs::set_permissions(raw_path, perms);
            }
        }

        for attempt in 0..15 {
            let handle = unsafe { BeginUpdateResourceW(wide_path.as_ptr(), 0) };
            if !handle.is_null() {
                return Ok(handle);
            }
            std::thread::sleep(std::time::Duration::from_millis(30 * (attempt as u64 + 1)));
        }

        Err(format!(
            "BeginUpdateResourceW failed for {} (file locked by another process)",
            raw_path.display()
        ))
    }

    pub fn set_metadata(
        raw_exe_path: &Path,
        title: &str,
        rpc_app_id: Option<&str>,
        png: Option<&[u8]>,
    ) -> Result<(), String> {
        let title_bytes = title.as_bytes();
        let exe_path = wide_path(raw_exe_path);
        let png = png.filter(|bytes| super::is_valid_icon_png(bytes));
        let group_icon = png.map(|bytes| build_group_icon(bytes.len()));

        let handle = begin_update_resource_with_retry(raw_exe_path, &exe_path)?;

        unsafe {
            let mut ok = false;
            let result = (|| {
                if !title_bytes.is_empty()
                    && UpdateResourceW(
                        handle,
                        make_int_resource(10),
                        make_int_resource(101),
                        0,
                        title_bytes.as_ptr() as *const c_void,
                        title_bytes.len() as u32,
                    ) == 0
                {
                    return Err("UpdateResourceW RT_RCDATA title failed".to_string());
                }

                if let Some(app_id) = rpc_app_id {
                    if UpdateResourceW(
                        handle,
                        make_int_resource(10),
                        make_int_resource(102),
                        0,
                        app_id.as_ptr() as *const c_void,
                        app_id.len() as u32,
                    ) == 0
                    {
                        return Err("RPC application ID resource could not be written".to_string());
                    }
                }

                if let (Some(png), Some(group_icon)) = (png, group_icon.as_ref()) {
                    if png.is_empty() {
                        return Err("empty icon".to_string());
                    }

                    if UpdateResourceW(
                        handle,
                        make_int_resource(3),
                        make_int_resource(1),
                        0,
                        png.as_ptr() as *const c_void,
                        png.len() as u32,
                    ) == 0
                    {
                        return Err("UpdateResourceW RT_ICON failed".to_string());
                    }

                    if UpdateResourceW(
                        handle,
                        make_int_resource(14),
                        make_int_resource(1),
                        0,
                        group_icon.as_ptr() as *const c_void,
                        group_icon.len() as u32,
                    ) == 0
                    {
                        return Err("UpdateResourceW RT_GROUP_ICON failed".to_string());
                    }
                }

                ok = true;
                Ok(())
            })();

            if EndUpdateResourceW(handle, if ok { 0 } else { 1 }) == 0 {
                return Err("EndUpdateResourceW failed".to_string());
            }

            result
        }
    }
}

static DISCORD_CLIENT: OnceCell<Mutex<Option<rpc::Client>>> = OnceCell::new();
static BULK_TASK_RUNNING: OnceCell<Mutex<bool>> = OnceCell::new();

#[derive(Default, Serialize)]
struct CleanupResult {
    deleted_files: u64,
    deleted_dirs: u64,
    bytes_freed: u64,
}

#[derive(Clone, Serialize)]
struct BulkTaskStarted {
    mode: String,
}

#[derive(Clone, Serialize)]
struct BulkTaskOutput {
    stream: String,
    line: String,
}

#[derive(Clone, Serialize)]
struct BulkTaskFinished {
    mode: String,
    success: bool,
    exit_code: Option<i32>,
    message: String,
}

#[derive(Clone, Serialize)]
struct BulkTaskAvailability {
    available: bool,
    message: String,
}

#[derive(Serialize)]
struct GeneratedExecutable {
    filename: String,
    relative_path: String,
    full_path: String,
    size: u64,
    modified: u64,
}

#[derive(Serialize)]
struct GeneratedGame {
    app_id: String,
    name: Option<String>,
    folder_path: String,
    exe_count: usize,
    size: u64,
    modified: u64,
    executables: Vec<GeneratedExecutable>,
}

#[derive(Serialize)]
struct GeneratedGameSummary {
    app_id: String,
    name: Option<String>,
    folder_path: String,
    modified: u64,
}

#[derive(Deserialize)]
struct GameProcessQuery {
    app_id: String,
    path: String,
    executable_name: String,
}

#[derive(Serialize)]
struct GameProcessStatus {
    app_id: String,
    path: String,
    executable_name: String,
    full_path: String,
    installed: bool,
    running: bool,
    process_ids: Vec<u32>,
}

#[derive(Serialize)]
struct RunningGeneratedGameProcess {
    app_id: String,
    path: String,
    executable_name: String,
    full_path: String,
    process_ids: Vec<u32>,
}

#[derive(Clone, Serialize, Deserialize)]
struct SearchPriorityItem {
    name: String,
    normalized_name: String,
    rank: u32,
    score: f64,
    source: String,
}

#[derive(Clone, Serialize, Deserialize)]
struct SearchPriorityData {
    updated_at: u64,
    sources: Vec<String>,
    items: Vec<SearchPriorityItem>,
}

#[derive(Serialize)]
struct SearchPriorityResponse {
    data: SearchPriorityData,
    refreshed: bool,
    used_cache: bool,
    error: Option<String>,
}

#[derive(Clone, Serialize, Deserialize)]
struct NewGameCatalogItem {
    id: String,
    name: String,
    icon_hash: Option<String>,
    executable_count: usize,
}

#[derive(Clone, Serialize, Deserialize)]
struct NewGameItem {
    id: String,
    name: String,
    icon_hash: Option<String>,
    executable_count: usize,
    detected_at: u64,
}

#[derive(Clone, Serialize, Deserialize)]
struct NewGamesMetadata {
    schema_version: u32,
    updated_at: u64,
    last_checked_at: u64,
    known_count: usize,
    known_ids: Vec<String>,
    new_games: Vec<NewGameItem>,
}

#[derive(Clone, Serialize)]
struct NewGamesState {
    updated_at: u64,
    last_checked_at: u64,
    known_count: usize,
    new_games: Vec<NewGameItem>,
}

#[derive(Serialize)]
struct NewGamesSyncResponse {
    state: NewGamesState,
    new_games: Vec<NewGameItem>,
}

#[derive(Clone, Deserialize)]
struct ActiveQuestGameExecutable {
    name: String,
    os: String,
    #[serde(default)]
    requires_risk_acceptance: bool,
}

#[derive(Clone, Deserialize)]
struct ActiveQuestGameCatalogItem {
    id: String,
    name: String,
    icon_hash: Option<String>,
    #[serde(default)]
    aliases: Vec<String>,
    #[serde(default)]
    executables: Vec<ActiveQuestGameExecutable>,
}

#[derive(Clone)]
struct ActiveQuestInfo {
    id: String,
    name: String,
    application_ids: Vec<String>,
    game_names: Vec<String>,
    expires_at: u64,
    play_task: bool,
    stream_task: bool,
}

#[derive(Serialize)]
struct ActiveQuestCreatedGame {
    id: String,
    name: String,
    quest_ids: Vec<String>,
    quest_names: Vec<String>,
    executable_count: usize,
    match_method: String,
    match_score: u32,
    fallback_executable_used: bool,
    has_play_quest: bool,
    has_stream_quest: bool,
}

#[derive(Serialize)]
struct ActiveQuestCreateResponse {
    active_quest_count: usize,
    matched_game_count: usize,
    created_executable_count: usize,
    skipped_executable_count: usize,
    created_games: Vec<ActiveQuestCreatedGame>,
    sources: Vec<String>,
    source_errors: Vec<String>,
    creation_errors: Vec<String>,
}

#[derive(Clone)]
struct ActiveQuestMatch {
    game: ActiveQuestGameCatalogItem,
    method: String,
    score: u32,
    fallback_executable_used: bool,
}

struct ActiveQuestMatchedGroup {
    game: ActiveQuestGameCatalogItem,
    quests: Vec<ActiveQuestInfo>,
    match_method: String,
    match_score: u32,
    fallback_executable_used: bool,
}

fn get_discord_client() -> &'static Mutex<Option<rpc::Client>> {
    DISCORD_CLIENT.get_or_init(|| Mutex::new(None))
}

fn get_bulk_task_running() -> &'static Mutex<bool> {
    BULK_TASK_RUNNING.get_or_init(|| Mutex::new(false))
}

fn get_app_dir() -> Result<PathBuf, String> {
    #[cfg(target_os = "windows")]
    {
        let base = env::var_os("LOCALAPPDATA")
            .filter(|value| !value.is_empty())
            .ok_or_else(|| "Windows local data directory is unavailable".to_string())?;
        let directory = PathBuf::from(base).join("DiscordQuestHelper");
        std::fs::create_dir_all(&directory)
            .map_err(|error| format!("Could not create application data directory: {error}"))?;
        Ok(directory)
    }
    #[cfg(not(target_os = "windows"))]
    {
        let exe_path =
            env::current_exe().map_err(|e| format!("Failed to get current exe: {}", e))?;
        Ok(exe_path
            .parent()
            .unwrap_or_else(|| Path::new(""))
            .to_path_buf())
    }
}

fn restore_embedded_file_if_missing(
    path: &Path,
    bytes: &[u8],
    label: &str,
) -> Result<bool, String> {
    if let Ok(metadata) = std::fs::metadata(path) {
        if metadata.is_file() && metadata.len() > 0 {
            return Ok(false);
        }
    }

    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("{label} klasörü oluşturulamadı: {e}"))?;
    }

    std::fs::write(path, bytes).map_err(|e| format!("{label} geri yazılamadı: {e}"))?;
    Ok(true)
}

fn write_embedded_file(path: &Path, bytes: &[u8], label: &str) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("{label} klasörü oluşturulamadı: {e}"))?;
    }

    std::fs::write(path, bytes).map_err(|e| format!("{label} yazılamadı: {e}"))
}

fn ensure_embedded_base_files(app_dir: &Path) -> Result<(), String> {
    std::fs::create_dir_all(app_dir.join("metadata"))
        .map_err(|e| format!("Metadata folder could not be created: {e}"))?;

    let runtime_icon = app_dir.join("resources").join("icon.ico");
    restore_embedded_file_if_missing(&runtime_icon, EMBEDDED_ICON_ICO, "Application icon")?;

    Ok(())
}

fn write_embedded_runner_template(app_dir: &Path) -> Result<PathBuf, String> {
    let runner_template = app_dir.join("data").join("src-win.exe");
    // Use the embedded runner rather than a template left by an older version.
    write_embedded_file(&runner_template, EMBEDDED_RUNNER_EXE, "Template exe")?;
    Ok(runner_template)
}

fn sanitize_filename(name: &str) -> String {
    let trimmed = name
        .trim()
        .trim_matches(|c| c == '"' || c == '\'' || c == '>' || c == '<');
    let mut cleaned: String = trimmed
        .chars()
        .map(|c| match c {
            '<' | '>' | ':' | '"' | '|' | '?' | '*' | '/' | '\\' => '_',
            c if c.is_control() => '_',
            c => c,
        })
        .collect();

    while cleaned.starts_with('_')
        || cleaned.starts_with('.')
        || cleaned.starts_with('>')
        || cleaned.starts_with('<')
    {
        cleaned.remove(0);
        if cleaned.is_empty() {
            break;
        }
    }
    let cleaned = cleaned.trim().to_string();

    // A path segment cannot resolve to the current or parent directory.
    if cleaned.is_empty() || cleaned.chars().all(|c| c == '.') {
        "game.exe".to_string()
    } else {
        cleaned
    }
}

fn safe_path_segment(value: &str, label: &str) -> Result<String, String> {
    let value = value.trim();
    if value.is_empty()
        || value == "."
        || value == ".."
        || value
            .chars()
            .any(|c| c.is_control() || "\\/:<>\"|?*".contains(c))
        || value.ends_with('.')
    {
        return Err(format!("Invalid {label}: {value}"));
    }
    let stem = value
        .split('.')
        .next()
        .unwrap_or_default()
        .to_ascii_uppercase();
    if matches!(stem.as_str(), "CON" | "PRN" | "AUX" | "NUL")
        || (stem.len() == 4
            && (stem.starts_with("COM") || stem.starts_with("LPT"))
            && matches!(stem.as_bytes()[3], b'1'..=b'9'))
    {
        return Err(format!("Reserved Windows filename in {label}"));
    }
    let sanitized = sanitize_filename(value);
    if sanitized.is_empty() {
        return Err(format!("Invalid {label}: {value}"));
    }
    Ok(sanitized)
}

fn safe_relative_path(value: &str, label: &str) -> Result<PathBuf, String> {
    let mut output = PathBuf::new();
    for component in Path::new(value).components() {
        match component {
            std::path::Component::Normal(part) => {
                output.push(safe_path_segment(&part.to_string_lossy(), label)?);
            }
            std::path::Component::CurDir => {}
            _ => return Err(format!("Invalid {label}: {value}")),
        }
    }
    Ok(output)
}

fn generated_game_folder(app_dir: &Path, app_id: &str) -> Result<PathBuf, String> {
    Ok(app_dir
        .join("games")
        .join(safe_path_segment(app_id, "app id")?))
}

fn rpc_application_for_source<'a>(path: &str, app_id: &'a str) -> Result<Option<&'a str>, String> {
    let source = path.split(['/', '\\']).next().unwrap_or_default();
    if !source.eq_ignore_ascii_case("2.Source") {
        return Ok(None);
    }
    if !app_id.bytes().all(|byte| byte.is_ascii_digit())
        || app_id.parse::<i64>().ok().filter(|id| *id > 0).is_none()
    {
        return Err("RPC için geçerli bir Discord uygulama ID'si gerekiyor.".to_string());
    }
    Ok(Some(app_id))
}

fn game_executable_path(
    app_dir: &Path,
    app_id: &str,
    executable_path: &str,
    executable_name: &str,
) -> Result<PathBuf, String> {
    Ok(generated_game_folder(app_dir, app_id)?
        .join(safe_relative_path(executable_path, "game path")?)
        .join(safe_path_segment(executable_name, "exe name")?))
}

fn value_at<'a>(value: &'a serde_json::Value, path: &[&str]) -> Option<&'a serde_json::Value> {
    let mut current = value;
    for key in path {
        current = current.get(*key)?;
    }
    Some(current)
}

fn string_at(value: &serde_json::Value, path: &[&str]) -> Option<String> {
    value_at(value, path)
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

fn timestamp_at(value: &serde_json::Value, paths: &[&[&str]]) -> Option<u64> {
    for path in paths {
        if let Some(timestamp) =
            string_at(value, path).and_then(|value| parse_rfc3339_unix_seconds(&value))
        {
            return Some(timestamp);
        }
    }
    None
}

fn push_unique(values: &mut Vec<String>, value: Option<String>) {
    let Some(value) = value else {
        return;
    };
    let value = value.trim();
    if value.is_empty() {
        return;
    }
    if !values.iter().any(|item| item.eq_ignore_ascii_case(value)) {
        values.push(value.to_string());
    }
}

fn parse_rfc3339_unix_seconds(value: &str) -> Option<u64> {
    let parsed =
        time::OffsetDateTime::parse(value.trim(), &time::format_description::well_known::Rfc3339)
            .ok()?;
    u64::try_from(parsed.unix_timestamp()).ok()
}

fn value_contains_desktop_task(value: &serde_json::Value) -> bool {
    match value {
        serde_json::Value::String(text) => {
            let normalized = text.to_ascii_uppercase();
            normalized.contains("PLAY_ON_DESKTOP")
                || normalized.contains("STREAM_ON_DESKTOP")
                || (normalized.contains("DESKTOP") && normalized.contains("PLAY"))
                || (normalized.contains("DESKTOP") && normalized.contains("STREAM"))
        }
        serde_json::Value::Array(items) => items.iter().any(value_contains_desktop_task),
        serde_json::Value::Object(map) => map.iter().any(|(key, value)| {
            value_contains_desktop_task(&serde_json::Value::String(key.clone()))
                || value_contains_desktop_task(value)
        }),
        _ => false,
    }
}

/// Which desktop task types a quest carries. A quest can carry both, and the
/// stream task needs the user to share the window in a voice channel, so the
/// interface keeps the two apart.
fn quest_task_kinds(value: &serde_json::Value) -> (bool, bool) {
    fn mark(text: &str, play: &mut bool, stream: &mut bool) {
        let normalized = text.to_ascii_uppercase();
        if normalized.contains("STREAM_ON_DESKTOP")
            || (normalized.contains("DESKTOP") && normalized.contains("STREAM"))
        {
            *stream = true;
        }
        if normalized.contains("PLAY_ON_DESKTOP")
            || (normalized.contains("DESKTOP") && normalized.contains("PLAY"))
        {
            *play = true;
        }
    }

    fn walk(value: &serde_json::Value, play: &mut bool, stream: &mut bool) {
        match value {
            serde_json::Value::String(text) => mark(text, play, stream),
            serde_json::Value::Array(items) => {
                for item in items {
                    walk(item, play, stream);
                }
            }
            serde_json::Value::Object(map) => {
                for (key, item) in map {
                    mark(key, play, stream);
                    walk(item, play, stream);
                }
            }
            _ => {}
        }
    }

    let mut play = false;
    let mut stream = false;
    walk(value, &mut play, &mut stream);
    (play, stream)
}

fn quest_items_from_value(value: &serde_json::Value) -> Vec<serde_json::Value> {
    if let Some(items) = value.as_array() {
        return items.clone();
    }
    for key in ["quests", "data", "items", "results"] {
        if let Some(items) = value.get(key).and_then(|value| value.as_array()) {
            return items.clone();
        }
        if let Some(map) = value.get(key).and_then(|value| value.as_object()) {
            return map.values().cloned().collect();
        }
    }
    if let Some(map) = value.as_object() {
        if map.values().all(|item| item.is_object()) {
            return map.values().cloned().collect();
        }
    }
    Vec::new()
}

fn collect_quest_application_ids(config: &serde_json::Value) -> Vec<String> {
    let mut ids = Vec::new();

    let task_roots = [
        value_at(config, &["task_config_v2", "tasks"]),
        value_at(config, &["taskConfigV2", "tasks"]),
        value_at(config, &["task_config", "tasks"]),
        value_at(config, &["taskConfig", "tasks"]),
        value_at(config, &["tasks"]),
    ];

    for root in task_roots.into_iter().flatten() {
        if let Some(tasks_obj) = root.as_object() {
            // First check desktop tasks specifically
            for task_key in ["PLAY_ON_DESKTOP", "STREAM_ON_DESKTOP"] {
                if let Some(task) = tasks_obj.get(task_key) {
                    if let Some(applications) = task.get("applications").and_then(|v| v.as_array())
                    {
                        for application in applications {
                            push_unique(&mut ids, string_at(application, &["id"]));
                        }
                    }
                }
            }

            // Also check any other tasks
            for task in tasks_obj.values() {
                if let Some(applications) = task.get("applications").and_then(|v| v.as_array()) {
                    for application in applications {
                        push_unique(&mut ids, string_at(application, &["id"]));
                    }
                }
            }
        }
    }

    if let Some(items) = config
        .get("supplementary_application_ids")
        .or_else(|| config.get("supplementaryApplicationIds"))
        .and_then(|value| value.as_array())
    {
        for item in items {
            push_unique(&mut ids, item.as_str().map(ToOwned::to_owned));
        }
    }

    push_unique(&mut ids, string_at(config, &["application", "id"]));
    push_unique(&mut ids, string_at(config, &["application_id"]));
    push_unique(&mut ids, string_at(config, &["applicationId"]));
    push_unique(&mut ids, string_at(config, &["developer_application_id"]));
    push_unique(&mut ids, string_at(config, &["developerApplicationId"]));

    ids
}

fn collect_quest_game_names(config: &serde_json::Value) -> Vec<String> {
    let mut names = Vec::new();
    push_unique(&mut names, string_at(config, &["application", "name"]));
    push_unique(&mut names, string_at(config, &["application_name"]));
    push_unique(&mut names, string_at(config, &["applicationName"]));
    push_unique(&mut names, string_at(config, &["messages", "game_title"]));
    push_unique(&mut names, string_at(config, &["messages", "gameTitle"]));
    push_unique(&mut names, string_at(config, &["game"]));
    push_unique(&mut names, string_at(config, &["gameName"]));
    push_unique(&mut names, string_at(config, &["game_title"]));
    push_unique(&mut names, string_at(config, &["name"]));
    push_unique(&mut names, string_at(config, &["title"]));
    push_unique(&mut names, string_at(config, &["messages", "quest_name"]));
    push_unique(&mut names, string_at(config, &["messages", "questName"]));

    let task_roots = [
        value_at(config, &["task_config_v2", "tasks"]),
        value_at(config, &["taskConfigV2", "tasks"]),
        value_at(config, &["task_config", "tasks"]),
        value_at(config, &["taskConfig", "tasks"]),
        value_at(config, &["tasks"]),
    ];

    for root in task_roots.into_iter().flatten() {
        if let Some(tasks_obj) = root.as_object() {
            for task in tasks_obj.values() {
                if let Some(applications) = task.get("applications").and_then(|v| v.as_array()) {
                    for application in applications {
                        push_unique(&mut names, string_at(application, &["name"]));
                        push_unique(&mut names, string_at(application, &["game_name"]));
                        push_unique(&mut names, string_at(application, &["title"]));
                    }
                }
            }
        }
    }

    names
}

fn quest_label_text(config: &serde_json::Value) -> String {
    [
        string_at(config, &["messages", "quest_name"]),
        string_at(config, &["messages", "questName"]),
        string_at(config, &["messages", "game_title"]),
        string_at(config, &["messages", "gameTitle"]),
        string_at(config, &["application", "name"]),
        string_at(config, &["application_name"]),
        string_at(config, &["name"]),
        string_at(config, &["title"]),
    ]
    .into_iter()
    .flatten()
    .collect::<Vec<_>>()
    .join(" ")
}

fn is_recent_quest_candidate(source: &str, starts_at: u64, expires_at: u64, now: u64) -> bool {
    if source == "Raw GitHub Feed" {
        expires_at <= now.saturating_add(QUEST_RAW_MAX_EXPIRY_SECONDS)
    } else {
        let _ = starts_at;
        true
    }
}

fn extract_active_desktop_quest(
    item: &serde_json::Value,
    source: &str,
    now: u64,
) -> Option<ActiveQuestInfo> {
    let config = item.get("config").unwrap_or(item);
    if !value_contains_desktop_task(config) {
        return None;
    }

    let id = string_at(item, &["id"])
        .or_else(|| string_at(config, &["id"]))
        .or_else(|| string_at(config, &["quest_id"]))?;
    let starts_at = timestamp_at(
        config,
        &[
            &["starts_at"],
            &["startsAt"],
            &["start_at"],
            &["startAt"],
            &["durationStart"],
        ],
    )
    .unwrap_or(0);
    let expires_at = timestamp_at(
        config,
        &[
            &["expires_at"],
            &["expiresAt"],
            &["ends_at"],
            &["endsAt"],
            &["end_at"],
            &["endAt"],
            &["durationEnd"],
        ],
    )?;

    // Allow active quests or upcoming quests starting within 7 days
    if now >= expires_at || starts_at > now.saturating_add(7 * 24 * 3600) {
        return None;
    }
    if !is_recent_quest_candidate(source, starts_at, expires_at, now) {
        return None;
    }

    let label = quest_label_text(config).to_ascii_lowercase();
    if label.contains("[test]") || label.contains("placeholder") {
        return None;
    }

    let application_ids = collect_quest_application_ids(config);
    let game_names = collect_quest_game_names(config);
    if application_ids.is_empty() && game_names.is_empty() {
        return None;
    }

    let name = string_at(config, &["messages", "quest_name"]).unwrap_or_else(|| id.clone());
    let (mut play_task, stream_task) = quest_task_kinds(config);
    // An unrecognised desktop task is treated as a play task so it still shows
    // in the main list; only a stream-only quest is moved aside.
    if !play_task && !stream_task {
        play_task = true;
    }

    Some(ActiveQuestInfo {
        id,
        name,
        application_ids,
        game_names,
        expires_at,
        play_task,
        stream_task,
    })
}

async fn fetch_quest_source(
    client: &reqwest::Client,
    label: &str,
    url: &str,
) -> Result<Vec<serde_json::Value>, String> {
    let response = client
        .get(url)
        .send()
        .await
        .map_err(|e| format!("{label}: quest list could not be fetched: {e}"))?;
    if !response.status().is_success() {
        return Err(format!(
            "{label}: quest list could not be fetched with HTTP {}",
            response.status()
        ));
    }
    let value = response
        .json::<serde_json::Value>()
        .await
        .map_err(|e| format!("{label}: quest list could not be read: {e}"))?;
    Ok(quest_items_from_value(&value))
}

async fn fetch_quest_detail(client: &reqwest::Client, quest_id: &str) -> Option<serde_json::Value> {
    let url = format!("{DISCORD_QUEST_DETAIL_API_BASE}{quest_id}");
    let response = client.get(url).send().await.ok()?;
    if !response.status().is_success() {
        return None;
    }
    response.json::<serde_json::Value>().await.ok()
}

async fn fetch_active_desktop_quests(
) -> Result<(Vec<ActiveQuestInfo>, Vec<String>, Vec<String>), String> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(25))
        .user_agent(concat!("DiscordQuestHelper/", env!("CARGO_PKG_VERSION")))
        .build()
        .map_err(|e| format!("Quest client could not be created: {e}"))?;
    let sources = [
        ("Public API", QUEST_PUBLIC_API_URL),
        ("Raw GitHub Feed", QUEST_RAW_GITHUB_URL),
    ];
    let now = now_unix_seconds();
    let mut quests_by_id: HashMap<String, ActiveQuestInfo> = HashMap::new();
    let mut used_sources = Vec::new();
    let mut source_errors = Vec::new();
    let mut pending_detail_ids: HashMap<String, String> = HashMap::new();

    for (label, url) in sources {
        match fetch_quest_source(&client, label, url).await {
            Ok(items) => {
                used_sources.push(label.to_string());
                for item in items {
                    let Some(quest) = extract_active_desktop_quest(&item, label, now) else {
                        continue;
                    };

                    if quest.application_ids.len() <= 1 || quest.game_names.is_empty() {
                        pending_detail_ids
                            .entry(quest.id.clone())
                            .or_insert_with(|| label.to_string());
                    }

                    quests_by_id
                        .entry(quest.id.clone())
                        .and_modify(|existing| {
                            for id in &quest.application_ids {
                                push_unique(&mut existing.application_ids, Some(id.clone()));
                            }
                            for name in &quest.game_names {
                                push_unique(&mut existing.game_names, Some(name.clone()));
                            }
                            existing.play_task |= quest.play_task;
                            existing.stream_task |= quest.stream_task;
                        })
                        .or_insert(quest);
                }
            }
            Err(error) => source_errors.push(error),
        }
    }

    if used_sources.is_empty() {
        return Err(if source_errors.is_empty() {
            "Active quest sources did not respond.".to_string()
        } else {
            source_errors.join("; ")
        });
    }

    if !pending_detail_ids.is_empty() {
        let semaphore = Arc::new(tokio::sync::Semaphore::new(6));
        let mut detail_tasks = Vec::new();

        for (quest_id, label) in pending_detail_ids {
            let Ok(permit) = semaphore.clone().acquire_owned().await else {
                break;
            };
            let client = client.clone();
            detail_tasks.push(tokio::spawn(async move {
                let _permit = permit;
                let detail = fetch_quest_detail(&client, &quest_id).await?;
                let quest = extract_active_desktop_quest(&detail, &label, now)?;
                Some((quest_id, quest))
            }));
        }

        for task in detail_tasks {
            let Ok(Some((quest_id, detail_quest))) = task.await else {
                continue;
            };

            if let Some(existing) = quests_by_id.get_mut(&quest_id) {
                for id in &detail_quest.application_ids {
                    push_unique(&mut existing.application_ids, Some(id.clone()));
                }
                for name in &detail_quest.game_names {
                    push_unique(&mut existing.game_names, Some(name.clone()));
                }
                existing.play_task |= detail_quest.play_task;
                existing.stream_task |= detail_quest.stream_task;
            }
        }
    }

    let mut quests = quests_by_id.into_values().collect::<Vec<_>>();
    quests.sort_by(|a, b| {
        a.expires_at
            .cmp(&b.expires_at)
            .then_with(|| a.name.cmp(&b.name))
            .then_with(|| a.id.cmp(&b.id))
    });

    Ok((quests, used_sources, source_errors))
}

fn normalize_game_lookup_text(value: &str) -> String {
    value
        .chars()
        .filter(|character| character.is_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

fn normalize_game_match_text(value: &str) -> String {
    let mut output = String::new();
    let mut previous_space = true;

    for character in value.chars() {
        if character.is_alphanumeric() {
            output.extend(character.to_lowercase());
            previous_space = false;
        } else if !previous_space {
            output.push(' ');
            previous_space = true;
        }
    }

    output.trim().to_string()
}

fn is_active_quest_stopword(value: &str) -> bool {
    matches!(
        value,
        "quest"
            | "gorev"
            | "görev"
            | "reward"
            | "odul"
            | "ödül"
            | "pack"
            | "event"
            | "desktop"
            | "launcher"
            | "game"
            | "free"
            | "demo"
            | "version"
            | "ver"
            | "play"
            | "oyna"
            | "oyun"
    )
}

fn meaningful_match_tokens(value: &str) -> HashSet<String> {
    normalize_game_match_text(value)
        .split_whitespace()
        .filter(|token| token.chars().count() >= 3 && !is_active_quest_stopword(token))
        .map(ToOwned::to_owned)
        .collect()
}

fn game_candidate_names(game: &ActiveQuestGameCatalogItem) -> Vec<&str> {
    std::iter::once(game.name.as_str())
        .chain(game.aliases.iter().map(String::as_str))
        .filter(|value| !value.trim().is_empty())
        .collect()
}

fn score_quest_name_against_game(
    quest_name: &str,
    game: &ActiveQuestGameCatalogItem,
) -> Option<(&'static str, u32)> {
    let quest_key = normalize_game_lookup_text(quest_name);
    let quest_text = normalize_game_match_text(quest_name);
    if quest_key.is_empty() || quest_text.is_empty() {
        return None;
    }

    for candidate_name in game_candidate_names(game) {
        let candidate_key = normalize_game_lookup_text(candidate_name);
        if !candidate_key.is_empty() && candidate_key == quest_key {
            return Some(("exact_name", 900));
        }
    }

    if quest_text.chars().count() >= 5 {
        for candidate_name in game_candidate_names(game) {
            let candidate_text = normalize_game_match_text(candidate_name);
            if candidate_text.chars().count() >= 5
                && (candidate_text.starts_with(&quest_text)
                    || quest_text.starts_with(&candidate_text))
            {
                return Some(("prefix_name", 780));
            }
        }
    }

    let quest_tokens = meaningful_match_tokens(quest_name);
    if !quest_tokens.is_empty() {
        for candidate_name in game_candidate_names(game) {
            let candidate_tokens = meaningful_match_tokens(candidate_name);
            let shared_tokens = quest_tokens
                .intersection(&candidate_tokens)
                .cloned()
                .collect::<Vec<_>>();
            if shared_tokens.len() >= 2 {
                let score = 660 + (shared_tokens.len().min(5) as u32 * 10);
                return Some(("token_overlap", score));
            }
        }
    }

    if quest_key.chars().count() >= 8 {
        for candidate_name in game_candidate_names(game) {
            let candidate_key = normalize_game_lookup_text(candidate_name);
            if candidate_key.chars().count() >= 8
                && (candidate_key.contains(&quest_key) || quest_key.contains(&candidate_key))
            {
                return Some(("contains_name", 600));
            }
        }
    }

    None
}

fn better_active_quest_match(
    current: Option<ActiveQuestMatch>,
    candidate: ActiveQuestMatch,
) -> ActiveQuestMatch {
    match current {
        Some(existing)
            if existing.score > candidate.score
                || (existing.score == candidate.score
                    && existing.game.name <= candidate.game.name) =>
        {
            existing
        }
        _ => candidate,
    }
}

fn build_active_game_maps(
    games: Vec<ActiveQuestGameCatalogItem>,
) -> (
    HashMap<String, ActiveQuestGameCatalogItem>,
    HashMap<String, String>,
) {
    let mut by_id = HashMap::new();
    let mut by_name = HashMap::new();

    for game in games {
        let id = game.id.trim();
        if id.is_empty() {
            continue;
        }

        let id = id.to_string();
        let name_key = normalize_game_lookup_text(&game.name);
        if !name_key.is_empty() {
            by_name.entry(name_key).or_insert_with(|| id.clone());
        }
        for alias in &game.aliases {
            let alias_key = normalize_game_lookup_text(alias);
            if !alias_key.is_empty() {
                by_name.entry(alias_key).or_insert_with(|| id.clone());
            }
        }

        by_id.entry(id).or_insert(game);
    }

    (by_id, by_name)
}

fn game_has_valid_desktop_executable(game: &ActiveQuestGameCatalogItem) -> bool {
    game.executables.iter().any(|exe| {
        let os = exe.os.trim().to_ascii_lowercase();
        (os.is_empty() || os == "win32")
            && !exe.requires_risk_acceptance
            && split_catalog_executable(exe).is_some()
    })
}

fn match_active_quest_game(
    quest: &ActiveQuestInfo,
    by_id: &HashMap<String, ActiveQuestGameCatalogItem>,
    by_name: &HashMap<String, String>,
) -> Option<ActiveQuestMatch> {
    // Prefer an explicit application ID with a supported Windows executable.
    for app_id in &quest.application_ids {
        if let Some(game) = by_id.get(app_id) {
            if game_has_valid_desktop_executable(game) {
                return Some(ActiveQuestMatch {
                    game: game.clone(),
                    method: "app_id".to_string(),
                    score: 1000,
                    fallback_executable_used: false,
                });
            }
        }
    }

    // Explicit application IDs must match a real Windows executable.
    if !quest.application_ids.is_empty() {
        return None;
    }

    // Match by name only when the quest supplies no application IDs.
    let mut query_names = quest
        .game_names
        .iter()
        .map(String::as_str)
        .collect::<Vec<_>>();
    if !quest.name.trim().is_empty() {
        query_names.push(quest.name.as_str());
    }

    let mut best_match_with_exe: Option<ActiveQuestMatch> = None;

    for name in &query_names {
        let key = normalize_game_lookup_text(name);
        if let Some(game) = by_name.get(&key).and_then(|game_id| by_id.get(game_id)) {
            let candidate = ActiveQuestMatch {
                game: game.clone(),
                method: "exact_name".to_string(),
                score: 900,
                fallback_executable_used: false,
            };
            if game_has_valid_desktop_executable(game) {
                best_match_with_exe =
                    Some(better_active_quest_match(best_match_with_exe, candidate));
            }
        }

        for game in by_id.values() {
            if let Some((method, score)) = score_quest_name_against_game(name, game) {
                let candidate = ActiveQuestMatch {
                    game: game.clone(),
                    method: method.to_string(),
                    score,
                    fallback_executable_used: false,
                };
                if game_has_valid_desktop_executable(game) {
                    best_match_with_exe =
                        Some(better_active_quest_match(best_match_with_exe, candidate));
                }
            }
        }
    }

    if let Some(match_found) = best_match_with_exe {
        return Some(match_found);
    }

    None
}

fn split_catalog_executable(executable: &ActiveQuestGameExecutable) -> Option<(String, String)> {
    let os = executable.os.trim().to_ascii_lowercase();
    if !(os.is_empty() || os == "win32") {
        return None;
    }

    let parts = executable
        .name
        .split(['\\', '/'])
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>();
    let filename = safe_path_segment(parts.last().copied()?, "exe name").ok()?;
    if !filename.to_ascii_lowercase().ends_with(".exe") {
        return None;
    }
    let relative_path = if parts.len() > 1 {
        parts[..parts.len() - 1].join("\\")
    } else {
        String::new()
    };
    safe_relative_path(&relative_path, "game path").ok()?;

    Some((relative_path, filename))
}

fn normalize_path_text(path: &Path) -> String {
    std::fs::canonicalize(path)
        .unwrap_or_else(|_| path.to_path_buf())
        .to_string_lossy()
        .trim_start_matches(r"\\?\")
        .replace('/', r"\")
        .to_ascii_lowercase()
}

fn metadata_dir() -> Result<PathBuf, String> {
    Ok(get_app_dir()?.join("metadata"))
}

fn search_priority_path() -> Result<PathBuf, String> {
    Ok(metadata_dir()?.join("search-priority.json"))
}

fn new_games_path() -> Result<PathBuf, String> {
    Ok(metadata_dir()?.join("new-games.json"))
}

fn collect_path_stats(path: &Path, stats: &mut CleanupResult) {
    let Ok(metadata) = std::fs::metadata(path) else {
        return;
    };

    if metadata.is_file() {
        stats.deleted_files += 1;
        stats.bytes_freed += metadata.len();
        return;
    }

    if metadata.is_dir() {
        stats.deleted_dirs += 1;
        if let Ok(entries) = std::fs::read_dir(path) {
            for entry in entries.flatten() {
                collect_path_stats(&entry.path(), stats);
            }
        }
    }
}

fn remove_path(path: &Path) -> Result<CleanupResult, String> {
    let mut stats = CleanupResult::default();

    if !path.exists() {
        return Ok(stats);
    }

    collect_path_stats(path, &mut stats);
    if path.is_dir() {
        std::fs::remove_dir_all(path)
            .map_err(|e| format!("Failed to remove directory {:?}: {}", path, e))?;
    } else {
        std::fs::remove_file(path)
            .map_err(|e| format!("Failed to remove file {:?}: {}", path, e))?;
    }

    Ok(stats)
}

fn system_time_to_unix_seconds(time: std::time::SystemTime) -> u64 {
    time.duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

fn now_unix_seconds() -> u64 {
    system_time_to_unix_seconds(SystemTime::now())
}

fn collect_executables(
    root: &Path,
    current: &Path,
    executables: &mut Vec<GeneratedExecutable>,
) -> Result<(), String> {
    let entries = std::fs::read_dir(current).map_err(|e| {
        format!(
            "Failed to read generated game directory {:?}: {}",
            current, e
        )
    })?;

    for entry in entries {
        let entry = match entry {
            Ok(e) => e,
            Err(_) => continue,
        };
        let entry_path = entry.path();

        if entry_path.is_dir() {
            let _ = collect_executables(root, &entry_path, executables);
            continue;
        }

        if entry_path
            .extension()
            .and_then(|extension| extension.to_str())
            .map(|extension| extension.eq_ignore_ascii_case("exe"))
            != Some(true)
        {
            continue;
        }

        let metadata = match entry.metadata() {
            Ok(m) => m,
            Err(_) => continue,
        };
        let relative_path = entry_path
            .strip_prefix(root)
            .unwrap_or(&entry_path)
            .to_string_lossy()
            .to_string();

        executables.push(GeneratedExecutable {
            filename: entry_path
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string(),
            relative_path,
            full_path: entry_path.to_string_lossy().to_string(),
            size: metadata.len(),
            modified: metadata
                .modified()
                .map(system_time_to_unix_seconds)
                .unwrap_or_default(),
        });
    }

    Ok(())
}

fn read_generated_game_names(app_dir: &Path) -> HashMap<String, String> {
    let mut names = std::fs::read_to_string(app_dir.join("oyunlar.txt"))
        .unwrap_or_default()
        .lines()
        .filter_map(|line| {
            let (name, id) = line.rsplit_once(" - ")?;
            Some((id.trim().to_string(), name.trim().to_string()))
        })
        .collect::<HashMap<_, _>>();
    if let Ok(bytes) = std::fs::read(app_dir.join("metadata").join("game-names.json")) {
        if let Ok(current) = serde_json::from_slice::<HashMap<String, String>>(&bytes) {
            names.extend(current);
        }
    }
    names
}

fn store_generated_game_names(
    app_dir: &Path,
    updates: HashMap<String, String>,
) -> Result<(), String> {
    static NAMES_LOCK: Mutex<()> = Mutex::new(());
    let _guard = NAMES_LOCK
        .lock()
        .map_err(|_| "Game name storage unavailable")?;
    let mut names = read_generated_game_names(app_dir);
    names.extend(updates);
    let dir = app_dir.join("metadata");
    std::fs::create_dir_all(&dir).map_err(|error| error.to_string())?;
    let bytes = serde_json::to_vec_pretty(&names).map_err(|error| error.to_string())?;
    std::fs::write(dir.join("game-names.json"), bytes).map_err(|error| error.to_string())
}

fn is_valid_icon_png(bytes: &[u8]) -> bool {
    bytes.len() >= 33
        && bytes.starts_with(b"\x89PNG\r\n\x1a\n")
        && bytes[8..12] == 13u32.to_be_bytes()
        && &bytes[12..16] == b"IHDR"
        && u32::from_be_bytes(bytes[16..20].try_into().unwrap_or_default()) > 0
        && u32::from_be_bytes(bytes[20..24].try_into().unwrap_or_default()) > 0
}

fn validate_bulk_mode(mode: &str) -> Result<(), String> {
    match mode {
        "all" | "missing" | "icons" | "missing-icons" => Ok(()),
        _ => Err(format!("Unknown bulk task mode: {mode}")),
    }
}

fn build_generated_game_details(
    app_dir: &Path,
    app_id: &str,
    game_names: &std::collections::HashMap<String, String>,
) -> Result<GeneratedGame, String> {
    let app_id = safe_path_segment(app_id, "app id")?;
    let game_path = generated_game_folder(app_dir, &app_id)?;

    if !game_path.exists() || !game_path.is_dir() {
        return Err("Generated game folder does not exist".to_string());
    }

    let mut executables = Vec::new();
    collect_executables(&game_path, &game_path, &mut executables)?;

    let size = executables.iter().map(|executable| executable.size).sum();
    let modified = executables
        .iter()
        .map(|executable| executable.modified)
        .max()
        .unwrap_or_else(|| {
            game_path
                .metadata()
                .ok()
                .and_then(|metadata| metadata.modified().ok())
                .map(system_time_to_unix_seconds)
                .unwrap_or_default()
        });

    executables.sort_by(|a, b| a.relative_path.cmp(&b.relative_path));

    Ok(GeneratedGame {
        name: game_names.get(&app_id).cloned(),
        folder_path: game_path.to_string_lossy().to_string(),
        exe_count: executables.len(),
        size,
        modified,
        app_id,
        executables,
    })
}

fn emit_bulk_output(handle: &AppHandle, stream: &str, line: String) {
    let _ = handle.emit(
        "bulk_game_task_output",
        BulkTaskOutput {
            stream: stream.to_string(),
            line,
        },
    );
}

fn normalize_priority_name(value: &str) -> String {
    let mut normalized = String::with_capacity(value.len());
    let mut previous_was_space = true;

    for character in value
        .replace('&', " and ")
        .replace(['©', '®', '™'], "")
        .chars()
    {
        if character.is_alphanumeric() {
            normalized.extend(character.to_lowercase());
            previous_was_space = false;
        } else if !previous_was_space {
            normalized.push(' ');
            previous_was_space = true;
        }
    }

    normalized.trim().to_string()
}

fn decode_common_html_entities(value: &str) -> String {
    value
        .replace("&amp;", "&")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&apos;", "'")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
}

fn normalize_search_priority_target_count(target_count: Option<u32>) -> u32 {
    target_count
        .unwrap_or(SEARCH_PRIORITY_TARGET_DEFAULT_COUNT)
        .clamp(
            SEARCH_PRIORITY_TARGET_MIN_COUNT,
            SEARCH_PRIORITY_TARGET_MAX_COUNT,
        )
}

fn insert_priority_item(
    items: &mut HashMap<String, SearchPriorityItem>,
    name: String,
    rank: u32,
    score: f64,
    source: &str,
) {
    let normalized_name = normalize_priority_name(&name);
    if normalized_name.is_empty() {
        return;
    }

    items
        .entry(normalized_name.clone())
        .and_modify(|item| {
            item.rank = item.rank.min(rank);
            item.score = item.score.max(score);
            if !item.source.split(',').any(|part| part == source) {
                item.source.push(',');
                item.source.push_str(source);
            }
        })
        .or_insert(SearchPriorityItem {
            name,
            normalized_name,
            rank,
            score,
            source: source.to_string(),
        });
}

#[derive(Deserialize)]
struct SteamSearchResults {
    results_html: String,
}

#[derive(Deserialize)]
struct SteamChartsResponse {
    response: SteamChartsPayload,
}

#[derive(Deserialize)]
struct SteamChartsPayload {
    ranks: Vec<SteamChartRank>,
}

#[derive(Deserialize)]
struct SteamChartRank {
    rank: u32,
    appid: u64,
}

async fn fetch_steam_top_seller_names(
    client: &reqwest::Client,
    target_count: u32,
) -> Result<Vec<(String, u32)>, String> {
    let title_regex = Regex::new(r#"<span class="title">([^<]+)</span>"#)
        .map_err(|e| format!("Failed to compile title parser: {}", e))?;
    let mut names = Vec::new();
    let mut last_error = None;
    let target_count = target_count.min(STEAM_TOP_SELLERS_MAX_COUNT);

    for start in (0..target_count).step_by(STEAM_SEARCH_PAGE_SIZE as usize) {
        if start > 0 {
            tokio::time::sleep(Duration::from_millis(STEAM_TOP_SELLERS_PAGE_DELAY_MS)).await;
        }

        let remaining = target_count - start;
        let count = STEAM_SEARCH_PAGE_SIZE.min(remaining);
        let url = format!(
            "https://store.steampowered.com/search/results/?query=&start={start}&count={count}&filter=globaltopsellers&infinite=1"
        );

        let mut attempt = 0;
        let response = loop {
            match client
                .get(&url)
                .header("User-Agent", "Mozilla/5.0")
                .header("Accept-Language", "en-US,en;q=0.9")
                .send()
                .await
            {
                Ok(response) => {
                    let status = response.status();
                    if status.as_u16() == 429 && attempt < STEAM_TOP_SELLERS_MAX_PAGE_RETRIES {
                        attempt += 1;
                        tokio::time::sleep(Duration::from_millis(
                            STEAM_TOP_SELLERS_RATE_LIMIT_DELAY_MS * attempt as u64,
                        ))
                        .await;
                        continue;
                    }

                    if !status.is_success() {
                        last_error = Some(if status.as_u16() == 429 {
                            "Steam top sellers is rate limiting requests".to_string()
                        } else {
                            format!("Steam top sellers returned an error: HTTP status {status}")
                        });
                        break None;
                    }

                    break Some(response);
                }
                Err(error) => {
                    if attempt < STEAM_TOP_SELLERS_MAX_PAGE_RETRIES {
                        attempt += 1;
                        tokio::time::sleep(Duration::from_millis(
                            STEAM_TOP_SELLERS_RATE_LIMIT_DELAY_MS * attempt as u64,
                        ))
                        .await;
                        continue;
                    }

                    last_error = Some(format!("Failed to fetch Steam top sellers: {}", error));
                    break None;
                }
            }
        };

        let Some(response) = response else {
            break;
        };

        let response = match response.json::<SteamSearchResults>().await {
            Ok(response) => response,
            Err(error) => {
                last_error = Some(format!("Failed to parse Steam top sellers: {}", error));
                break;
            }
        };

        let page_names = title_regex
            .captures_iter(&response.results_html)
            .enumerate()
            .filter_map(|(index, capture)| {
                capture.get(1).map(|title| {
                    (
                        decode_common_html_entities(title.as_str())
                            .trim()
                            .to_string(),
                        start + index as u32 + 1,
                    )
                })
            })
            .filter(|(name, _rank)| !name.is_empty())
            .collect::<Vec<_>>();

        if page_names.is_empty() {
            break;
        }

        let page_count = page_names.len();
        names.extend(page_names);
        if names.len() >= target_count as usize {
            break;
        }
        if page_count < count as usize {
            break;
        }
    }

    names.truncate(target_count as usize);

    if names.is_empty() {
        Err(last_error.unwrap_or_else(|| "Steam top sellers returned no games".to_string()))
    } else {
        Ok(names)
    }
}

async fn fetch_steam_app_name(client: &reqwest::Client, appid: u64) -> Option<String> {
    let url = format!("https://store.steampowered.com/api/appdetails?appids={appid}&filters=basic");
    let response = client
        .get(url)
        .header("User-Agent", "Mozilla/5.0")
        .send()
        .await
        .ok()?
        .error_for_status()
        .ok()?
        .json::<serde_json::Value>()
        .await
        .ok()?;

    let appid_key = appid.to_string();
    response
        .get(&appid_key)
        .and_then(|entry| entry.get("data"))
        .and_then(|data| data.get("name"))
        .and_then(|name| name.as_str())
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .map(ToString::to_string)
}

async fn fetch_steam_most_played_names(
    client: &reqwest::Client,
    target_count: u32,
) -> Result<Vec<(String, u32)>, String> {
    let response = client
        .get(format!(
            "https://api.steampowered.com/ISteamChartsService/GetMostPlayedGames/v1/?count={target_count}"
        ))
        .header("User-Agent", "Mozilla/5.0")
        .send()
        .await
        .map_err(|e| format!("Failed to fetch Steam most played games: {}", e))?
        .error_for_status()
        .map_err(|e| format!("Steam most played returned an error: {}", e))?
        .json::<SteamChartsResponse>()
        .await
        .map_err(|e| format!("Failed to parse Steam most played games: {}", e))?;

    let ranks = response
        .response
        .ranks
        .into_iter()
        .take(target_count as usize)
        .collect::<Vec<_>>();

    let mut names = Vec::new();
    for rank in ranks {
        if let Some(name) = fetch_steam_app_name(client, rank.appid).await {
            names.push((name, rank.rank));
        }
        tokio::time::sleep(Duration::from_millis(STEAM_APPDETAILS_DELAY_MS)).await;
    }

    if names.is_empty() {
        Err("Steam most played returned no matching app names".to_string())
    } else {
        Ok(names)
    }
}

async fn fetch_search_priority_data(target_count: u32) -> Result<SearchPriorityData, String> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(30))
        .build()
        .map_err(|e| format!("Failed to create HTTP client: {}", e))?;
    let mut items: HashMap<String, SearchPriorityItem> = HashMap::new();
    let mut sources = Vec::new();
    let mut errors = Vec::new();

    match fetch_steam_top_seller_names(&client, target_count).await {
        Ok(names) => {
            sources.push("steam_global_top_sellers".to_string());
            for (name, rank) in names {
                insert_priority_item(
                    &mut items,
                    name,
                    rank,
                    (20_000.0 - rank as f64 * 5.0).max(100.0),
                    "steam_global_top_sellers",
                );
            }
        }
        Err(error) => errors.push(error),
    }

    match fetch_steam_most_played_names(&client, target_count).await {
        Ok(names) => {
            sources.push("steam_most_played".to_string());
            for (name, rank) in names {
                insert_priority_item(
                    &mut items,
                    name,
                    rank,
                    (18_000.0 - rank as f64 * 4.5).max(100.0),
                    "steam_most_played",
                );
            }
        }
        Err(error) => errors.push(error),
    }

    if items.is_empty() {
        return Err(if errors.is_empty() {
            "No priority sources returned data".to_string()
        } else {
            errors.join("; ")
        });
    }

    let mut items: Vec<SearchPriorityItem> = items.into_values().collect();
    items.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.rank.cmp(&b.rank))
            .then_with(|| a.name.cmp(&b.name))
    });
    items.truncate(target_count as usize);

    Ok(SearchPriorityData {
        updated_at: now_unix_seconds(),
        sources,
        items,
    })
}

fn read_cached_search_priority() -> Option<SearchPriorityData> {
    let path = match search_priority_path() {
        Ok(p) => p,
        Err(e) => {
            eprintln!("Search priority cache path error: {}", e);
            return None;
        }
    };
    let bytes = match std::fs::read(&path) {
        Ok(b) => b,
        Err(e) => {
            eprintln!(
                "Search priority cache read error ({}): {}",
                path.display(),
                e
            );
            return None;
        }
    };
    match serde_json::from_slice(&bytes) {
        Ok(data) => Some(data),
        Err(e) => {
            eprintln!("Search priority cache parse error: {}", e);
            None
        }
    }
}

fn empty_search_priority_data() -> SearchPriorityData {
    SearchPriorityData {
        updated_at: 0,
        sources: Vec::new(),
        items: Vec::new(),
    }
}

fn empty_new_games_metadata() -> NewGamesMetadata {
    NewGamesMetadata {
        schema_version: 1,
        updated_at: 0,
        last_checked_at: 0,
        known_count: 0,
        known_ids: Vec::new(),
        new_games: Vec::new(),
    }
}

fn read_new_games_metadata() -> NewGamesMetadata {
    let Some(path) = new_games_path().ok() else {
        return empty_new_games_metadata();
    };
    let Ok(bytes) = std::fs::read(path) else {
        return empty_new_games_metadata();
    };
    let mut metadata = serde_json::from_slice::<NewGamesMetadata>(&bytes)
        .unwrap_or_else(|_| empty_new_games_metadata());

    if metadata.schema_version != 1 {
        metadata = empty_new_games_metadata();
    }

    metadata
}

fn new_games_state(metadata: &NewGamesMetadata) -> NewGamesState {
    NewGamesState {
        updated_at: metadata.updated_at,
        last_checked_at: metadata.last_checked_at,
        known_count: metadata.known_count,
        new_games: metadata.new_games.clone(),
    }
}

fn write_new_games_metadata(metadata: &NewGamesMetadata) -> Result<(), String> {
    let path = new_games_path()?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("Failed to create metadata folder: {}", e))?;
    }

    let bytes = serde_json::to_vec_pretty(metadata)
        .map_err(|e| format!("Failed to serialize new games metadata: {}", e))?;
    std::fs::write(&path, bytes).map_err(|e| format!("Failed to write new games metadata: {}", e))
}

fn normalize_game_catalog_item(item: NewGameCatalogItem) -> Option<NewGameCatalogItem> {
    let id = item.id.trim().to_string();
    if id.is_empty() {
        return None;
    }

    let name = item.name.trim().to_string();
    Some(NewGameCatalogItem {
        id: id.clone(),
        name: if name.is_empty() { id } else { name },
        icon_hash: item.icon_hash.filter(|value| !value.trim().is_empty()),
        executable_count: item.executable_count,
    })
}

fn run_bulk_game_process(handle: AppHandle, app_dir: PathBuf, mode: String) -> Result<i32, String> {
    emit_bulk_output(
        &handle,
        "stdout",
        format!("Starting native bulk game process (mode: {})...", mode),
    );

    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .map_err(|e| format!("Failed to build tokio runtime: {}", e))?;

    runtime.block_on(async move {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(30))
            .build()
            .map_err(|e| e.to_string())?;

        emit_bulk_output(&handle, "stdout", "Fetching games list...".to_string());

        let games_url = "https://discord.com/api/applications/detectable";
        let response = match client
            .get(games_url)
            .send()
            .await
            .and_then(|r| r.error_for_status())
        {
            Ok(res) => res,
            Err(e) => return Err(e.to_string()),
        };
        let games: Vec<ActiveQuestGameCatalogItem> = match response.json().await {
            Ok(json) => json,
            Err(e) => return Err(e.to_string()),
        };

        let mode_norm = mode.trim().to_ascii_lowercase();
        let is_missing = mode_norm == "missing";
        let is_icons = mode_norm == "icons";
        let is_missing_icons = mode_norm == "missing-icons" || mode_norm == "missing_icons";

        struct Task {
            game_id: String,
            game_name: String,
            dest_dir: PathBuf,
            dest_file: PathBuf,
            icon_hash: Option<String>,
            file_exists: bool,
        }

        let mut tasks = Vec::new();
        for game in games {
            if game.id.trim().is_empty() {
                continue;
            }
            let Ok(safe_game_id) = safe_path_segment(&game.id, "app id") else {
                continue;
            };
            for exe in game.executables {
                let Some((relative_path, filename)) = split_catalog_executable(&exe) else {
                    continue;
                };
                let dest_dir = app_dir
                    .join("games")
                    .join(&safe_game_id)
                    .join("1.Source")
                    .join(safe_relative_path(&relative_path, "game path")?);
                let dest_file = dest_dir.join(&filename);
                let file_exists = dest_file.exists();

                // Missing-only mode keeps existing executables.
                if is_missing && file_exists {
                    continue;
                }

                // Icon-only modes update existing games without creating executables.
                if (is_icons || is_missing_icons) && !file_exists {
                    continue;
                }

                tasks.push(Task {
                    game_id: game.id.clone(),
                    game_name: game.name.clone(),
                    dest_dir,
                    dest_file,
                    icon_hash: game.icon_hash.clone(),
                    file_exists,
                });
            }
        }

        emit_bulk_output(
            &handle,
            "stdout",
            format!(
                "Found {} game executables to process (mode: {})",
                tasks.len(),
                mode
            ),
        );

        let template_path = app_dir.join("data").join("src-win.exe");
        if !template_path.exists() && (!is_icons && !is_missing_icons) {
            return Err("Template data/src-win.exe not found".to_string());
        }

        let semaphore = std::sync::Arc::new(tokio::sync::Semaphore::new(96));
        let mut join_handles = Vec::new();

        for task in tasks {
            let client = client.clone();
            let template_path = template_path.clone();
            let semaphore = semaphore.clone();

            join_handles.push(tokio::spawn(async move {
                let _permit = semaphore.acquire().await.map_err(|e| e.to_string())?;

                if !task.file_exists || (!is_icons && !is_missing_icons && !is_missing) {
                    std::fs::create_dir_all(&task.dest_dir).map_err(|e| {
                        format!(
                            "Failed to create directory {}: {}",
                            task.dest_dir.display(),
                            e
                        )
                    })?;

                    std::fs::copy(&template_path, &task.dest_file).map_err(|e| {
                        format!(
                            "Failed to copy template to {}: {}",
                            task.dest_file.display(),
                            e
                        )
                    })?;
                }

                #[cfg(target_os = "windows")]
                if is_missing_icons && exe_icon::has_custom_icon(&task.dest_file, &template_path) {
                    return Ok::<_, String>((task.game_id, task.game_name));
                }
                let mut icon_bytes: Option<Vec<u8>> = None;
                if let Some(hash) = task.icon_hash {
                    let icon_url = format!(
                        "https://cdn.discordapp.com/app-icons/{}/{}.png?size=256",
                        task.game_id, hash
                    );
                    if let Ok(resp) = client
                        .get(&icon_url)
                        .send()
                        .await
                        .and_then(|r| r.error_for_status())
                    {
                        if let Ok(bytes) = resp.bytes().await {
                            if is_valid_icon_png(&bytes) {
                                icon_bytes = Some(bytes.to_vec());
                            }
                        }
                    }
                }

                if (is_icons || is_missing_icons) && icon_bytes.is_none() {
                    return Err(format!(
                        "Icon could not be downloaded for {}",
                        task.game_name
                    ));
                }
                #[cfg(target_os = "windows")]
                {
                    let icon_bytes_ref = icon_bytes.as_deref();
                    exe_icon::set_metadata(&task.dest_file, &task.game_name, None, icon_bytes_ref)
                        .map_err(|e| {
                            format!(
                                "Failed to set metadata for {}: {}",
                                task.dest_file.display(),
                                e
                            )
                        })?;
                }
                Ok::<_, String>((task.game_id, task.game_name))
            }));
        }

        let mut completed = 0;
        let mut saved_names = HashMap::new();
        let mut failed = 0;
        let total = join_handles.len();
        let activity_title = match mode_norm.as_str() {
            "missing" => "Eksik Oyunlar Oluşturuluyor",
            "icons" => "İkonlar Yenileniyor",
            "missing-icons" | "missing_icons" => "Eksik İkonlar Tamamlanıyor",
            _ => "Oyunlar Oluşturuluyor",
        };

        if total > 0 {
            emit_bulk_output(
                &handle,
                "stdout",
                format!("__GA_PROGRESS__|{}|0|0|{}", activity_title, total),
            );
        }

        for handle_task in join_handles {
            match handle_task.await {
                Ok(Ok((game_id, game_name))) => {
                    saved_names.insert(game_id, game_name);
                    completed += 1;
                    let percent = ((completed as f64 / total.max(1) as f64) * 100.0).round();
                    if completed % 25 == 0 || completed == total || completed == 1 {
                        emit_bulk_output(
                            &handle,
                            "stdout",
                            format!(
                                "__GA_PROGRESS__|{}|{}|{}|{}",
                                activity_title, percent, completed, total
                            ),
                        );
                    }
                }
                Ok(Err(e)) => {
                    failed += 1;
                    emit_bulk_output(&handle, "stderr", format!("Hata: {}", e));
                }
                Err(e) => {
                    failed += 1;
                    emit_bulk_output(&handle, "stderr", format!("Task panicked: {}", e));
                }
            }
        }

        emit_bulk_output(
            &handle,
            "stdout",
            "Finished processing all games.".to_string(),
        );
        store_generated_game_names(&app_dir, saved_names)?;
        if failed > 0 {
            return Err(format!(
                "{failed} game executables failed; {completed} succeeded"
            ));
        }
        Ok(0)
    })
}

#[allow(clippy::too_many_arguments)]
async fn create_fake_game_executable(
    app_dir: &Path,
    name: &str,
    path: &str,
    executable_name: &str,
    app_id: &str,
    icon_hash: &str,
) -> Result<String, String> {
    let rpc_application_id = rpc_application_for_source(path, app_id)?;
    let game_folder_path =
        generated_game_folder(app_dir, app_id)?.join(safe_relative_path(path, "game path")?);
    let executable_name = safe_path_segment(executable_name, "exe name")?;
    if !executable_name.to_ascii_lowercase().ends_with(".exe") {
        return Err("Only Windows .exe filenames are supported".to_string());
    }

    match std::fs::create_dir_all(&game_folder_path) {
        Ok(_) => {}
        Err(e) => return Err(format!("Failed to create game folder: {}", e)),
    };
    let target_executable_path = game_folder_path.join(&executable_name);

    match write_embedded_file(&target_executable_path, EMBEDDED_RUNNER_EXE, "Game exe") {
        Ok(()) => {
            let icon_url = if icon_hash.trim().is_empty() {
                None
            } else {
                Some(format!(
                    "https://cdn.discordapp.com/app-icons/{}/{}.png?size=256",
                    app_id, icon_hash
                ))
            };

            #[cfg(target_os = "windows")]
            {
                let icon_bytes = if let Some(icon_url) = icon_url {
                    // Use a dedicated client with an explicit timeout to avoid hanging
                    // game creation when the Discord CDN is slow or unresponsive.
                    let icon_client = reqwest::Client::builder()
                        .timeout(std::time::Duration::from_secs(10))
                        .build()
                        .map_err(|error| {
                            format!("Icon HTTP client could not be initialized: {error}")
                        })?;
                    match icon_client.get(&icon_url).send().await {
                        Ok(response) => match response.error_for_status() {
                            Ok(response) => response
                                .bytes()
                                .await
                                .ok()
                                .filter(|bytes| is_valid_icon_png(bytes)),
                            Err(_) => None,
                        },
                        Err(_) => None,
                    }
                } else {
                    None
                };

                let icon_bytes_ref = icon_bytes.as_ref().map(|bytes| bytes.as_ref());
                exe_icon::set_metadata(
                    &target_executable_path,
                    name,
                    rpc_application_id,
                    icon_bytes_ref,
                )?;
            }

            #[cfg(not(target_os = "windows"))]
            {
                if !name.is_empty() {
                    let _ = std::fs::write(game_folder_path.join("activity-title.txt"), name);
                }
            }

            store_generated_game_names(
                app_dir,
                HashMap::from([(app_id.to_string(), name.to_string())]),
            )?;
            Ok(format!(
                "Dummy executable copied to: {:?}",
                target_executable_path
            ))
        }
        Err(e) => Err(format!("Failed to create dummy executable: {}", e)),
    }
}

#[allow(clippy::too_many_arguments)]
#[tauri::command(rename_all = "snake_case")]
async fn create_fake_game(
    _handle: tauri::AppHandle,
    name: &str,
    path: &str,
    executable_name: &str,
    app_id: &str,
    icon_hash: &str,
) -> Result<String, String> {
    // Keep runtime files beside the portable executable.
    let exe_dir = get_app_dir()?;
    ensure_embedded_base_files(&exe_dir)?;
    let result =
        create_fake_game_executable(&exe_dir, name, path, executable_name, app_id, icon_hash)
            .await?;
    Ok(result)
}

#[tauri::command(rename_all = "snake_case")]
async fn create_active_quest_games(
    games: Vec<ActiveQuestGameCatalogItem>,
) -> Result<ActiveQuestCreateResponse, String> {
    if games.is_empty() {
        return Err("Game list is not ready.".to_string());
    }

    let app_dir = get_app_dir()?;
    ensure_embedded_base_files(&app_dir)?;

    let (by_id, by_name) = build_active_game_maps(games);
    if by_id.is_empty() {
        return Err("Discord game list to match is empty.".to_string());
    }

    let (active_quests, sources, source_errors) = fetch_active_desktop_quests().await?;
    let active_quest_count = active_quests.len();
    let mut matched_games: HashMap<String, ActiveQuestMatchedGroup> = HashMap::new();

    for quest in active_quests {
        let Some(game_match) = match_active_quest_game(&quest, &by_id, &by_name) else {
            continue;
        };

        let game_id = game_match.game.id.clone();
        if let Some(existing) = matched_games.get_mut(&game_id) {
            if game_match.score > existing.match_score {
                existing.match_method = game_match.method;
                existing.match_score = game_match.score;
            }
            existing.fallback_executable_used =
                existing.fallback_executable_used || game_match.fallback_executable_used;
            existing.quests.push(quest);
        } else {
            matched_games.insert(
                game_id,
                ActiveQuestMatchedGroup {
                    game: game_match.game,
                    quests: vec![quest],
                    match_method: game_match.method,
                    match_score: game_match.score,
                    fallback_executable_used: game_match.fallback_executable_used,
                },
            );
        }
    }

    let mut matched_games = matched_games.into_values().collect::<Vec<_>>();
    matched_games.sort_by(|a, b| {
        a.game
            .name
            .cmp(&b.game.name)
            .then_with(|| a.game.id.cmp(&b.game.id))
    });
    let matched_game_count = matched_games.len();

    let mut created_games = Vec::new();
    let mut created_executable_count = 0usize;
    let mut skipped_executable_count = 0usize;
    let mut creation_errors = Vec::new();

    for matched_game in matched_games {
        let game = matched_game.game;
        let mut executable_count = 0usize;
        for executable in &game.executables {
            let Some((path, executable_name)) = split_catalog_executable(executable) else {
                skipped_executable_count += 1;
                continue;
            };
            match create_fake_game_executable(
                &app_dir,
                &game.name,
                &format!("1.Source/{path}"),
                &executable_name,
                &game.id,
                game.icon_hash.as_deref().unwrap_or_default(),
            )
            .await
            {
                Ok(_) => {
                    executable_count += 1;
                    created_executable_count += 1;
                }
                Err(error) => {
                    skipped_executable_count += 1;
                    creation_errors.push(format!(
                        "{} / {} could not be created: {}",
                        game.name, executable_name, error
                    ));
                }
            }
        }

        if executable_count == 0 {
            continue;
        }

        created_games.push(ActiveQuestCreatedGame {
            id: game.id,
            name: game.name,
            quest_ids: matched_game
                .quests
                .iter()
                .map(|quest| quest.id.clone())
                .collect(),
            quest_names: matched_game
                .quests
                .iter()
                .map(|quest| quest.name.clone())
                .collect(),
            executable_count,
            match_method: matched_game.match_method,
            match_score: matched_game.match_score,
            fallback_executable_used: matched_game.fallback_executable_used,
            has_play_quest: matched_game.quests.iter().any(|quest| quest.play_task),
            has_stream_quest: matched_game.quests.iter().any(|quest| quest.stream_task),
        });
    }

    Ok(ActiveQuestCreateResponse {
        active_quest_count,
        matched_game_count,
        created_executable_count,
        skipped_executable_count,
        created_games,
        sources,
        source_errors,
        creation_errors,
    })
}

#[tauri::command(rename_all = "snake_case")]
fn get_game_process_statuses(
    requests: Vec<GameProcessQuery>,
) -> Result<Vec<GameProcessStatus>, String> {
    let app_dir = get_app_dir()?;
    let executable_paths: Vec<PathBuf> = requests
        .iter()
        .map(|request| {
            game_executable_path(
                &app_dir,
                &request.app_id,
                &request.path,
                &request.executable_name,
            )
        })
        .collect::<Result<_, _>>()?;

    #[cfg(target_os = "windows")]
    let process_ids_by_path = process_control::process_ids_for_paths(&executable_paths)?;

    #[cfg(not(target_os = "windows"))]
    let process_ids_by_path: Vec<Vec<u32>> = vec![Vec::new(); executable_paths.len()];

    Ok(requests
        .into_iter()
        .zip(executable_paths)
        .zip(process_ids_by_path)
        .map(
            |((request, executable_path), process_ids)| GameProcessStatus {
                app_id: request.app_id,
                path: request.path,
                executable_name: request.executable_name,
                full_path: executable_path.to_string_lossy().to_string(),
                installed: executable_path.exists(),
                running: !process_ids.is_empty(),
                process_ids,
            },
        )
        .collect())
}

#[tauri::command(rename_all = "snake_case")]
fn get_running_generated_game_processes() -> Result<Vec<RunningGeneratedGameProcess>, String> {
    let app_dir = get_app_dir()?;
    let games_dir = app_dir.join("games");

    #[cfg(target_os = "windows")]
    let running_processes = process_control::running_process_paths_under_dir(&games_dir)?;

    #[cfg(not(target_os = "windows"))]
    let running_processes: Vec<(u32, String)> = Vec::new();

    let games_root = {
        let normalized = normalize_path_text(&games_dir);
        if normalized.ends_with('\\') {
            normalized
        } else {
            format!("{normalized}\\")
        }
    };

    let mut by_path: std::collections::BTreeMap<String, RunningGeneratedGameProcess> =
        std::collections::BTreeMap::new();

    for (process_id, process_path) in running_processes {
        let Some(relative_path) = process_path.strip_prefix(&games_root) else {
            continue;
        };

        let segments: Vec<&str> = relative_path
            .split('\\')
            .filter(|segment| !segment.is_empty())
            .collect();

        if segments.len() < 2 {
            continue;
        }

        let app_id = segments[0].to_string();
        let executable_name = segments[segments.len() - 1].to_string();
        let path = if segments.len() > 2 {
            segments[1..segments.len() - 1].join("\\")
        } else {
            String::new()
        };

        by_path
            .entry(process_path.clone())
            .and_modify(|entry| entry.process_ids.push(process_id))
            .or_insert_with(|| RunningGeneratedGameProcess {
                app_id,
                path,
                executable_name,
                full_path: process_path,
                process_ids: vec![process_id],
            });
    }

    Ok(by_path.into_values().collect())
}

#[tauri::command(rename_all = "snake_case")]
async fn run_background_process(
    name: &str,
    path: &str,
    executable_name: &str,
    app_id: &str,
) -> Result<String, String> {
    let exe_dir = get_app_dir()?;
    let executable_path = game_executable_path(&exe_dir, app_id, path, executable_name)?;
    let game_folder_path = executable_path
        .parent()
        .ok_or_else(|| "Failed to resolve game folder path".to_string())?
        .to_path_buf();

    #[cfg(target_os = "windows")]
    if process_control::is_process_running_at_path(&executable_path)? {
        return Ok("Process is already running".to_string());
    }

    #[cfg(target_os = "windows")]
    if let Some(rpc_id) = rpc_application_for_source(path, app_id)? {
        // Refresh idle Source 2 binaries too: an installed EXE may predate the
        // RPC fix. Running processes above are left untouched.
        let icon = exe_icon::icon_image(&executable_path);
        write_embedded_file(&executable_path, EMBEDDED_RUNNER_EXE, "RPC runner")?;
        exe_icon::set_metadata(&executable_path, name, Some(rpc_id), icon.as_deref())?;
    }

    match std::process::Command::new(&executable_path)
        .args(["--title", name])
        .current_dir(game_folder_path) // Set working directory to the game folder
        .spawn()
    {
        Ok(_) => Ok("Process started successfully".to_string()),
        Err(e) => Err(format!("Failed to start process: {}", e)),
    }
}

#[tauri::command(rename_all = "snake_case")]
async fn stop_process(
    exec_name: String,
    app_id: Option<String>,
    path: Option<String>,
) -> Result<(), String> {
    if let Some(app_id) = app_id {
        let exe_dir = get_app_dir()?;
        let executable_path =
            game_executable_path(&exe_dir, &app_id, path.as_deref().unwrap_or(""), &exec_name)?;

        #[cfg(target_os = "windows")]
        {
            let _ = process_control::terminate_processes_at_path(&executable_path)?;
        }
        return Ok(());
    }

    Err(
        "The process can only be stopped with the full exe path in the generated game folder."
            .to_string(),
    )
}

#[tauri::command(rename_all = "snake_case")]
fn clear_icon_cache() -> Result<CleanupResult, String> {
    let cache_dir = get_app_dir()?.join("data").join("icons");
    remove_path(&cache_dir)
}

#[tauri::command(rename_all = "snake_case")]
fn clear_generated_games() -> Result<CleanupResult, String> {
    let app_dir = get_app_dir()?;
    let games_dir = app_dir.join("games");
    let mut stats = remove_path(&games_dir)?;
    let game_list_path = app_dir.join("oyunlar.txt");

    if game_list_path.exists() {
        collect_path_stats(&game_list_path, &mut stats);
        std::fs::remove_file(&game_list_path)
            .map_err(|e| format!("Failed to remove oyunlar.txt: {}", e))?;
    }

    Ok(stats)
}

#[tauri::command(rename_all = "snake_case")]
fn get_bulk_task_availability() -> Result<BulkTaskAvailability, String> {
    Ok(BulkTaskAvailability {
        available: true,
        message: "Bulk creation is ready.".to_string(),
    })
}

#[tauri::command(rename_all = "snake_case")]
async fn start_bulk_game_task(handle: AppHandle, mode: String) -> Result<(), String> {
    let mode = mode.trim().to_ascii_lowercase();
    let app_dir = get_app_dir()?;
    ensure_embedded_base_files(&app_dir)?;

    validate_bulk_mode(&mode)?;
    write_embedded_runner_template(&app_dir)?;

    {
        let mut running = get_bulk_task_running()
            .lock()
            .map_err(|_| "Bulk task state is unavailable".to_string())?;
        if *running {
            return Err("A bulk task is already running".to_string());
        }
        *running = true;
    }

    let _ = handle.emit(
        "bulk_game_task_started",
        BulkTaskStarted { mode: mode.clone() },
    );

    tauri::async_runtime::spawn_blocking(move || {
        let result = run_bulk_game_process(handle.clone(), app_dir, mode.clone());

        if let Ok(mut running) = get_bulk_task_running().lock() {
            *running = false;
        }

        let (success, exit_code, message) = match result {
            Ok(code) => (
                code == 0,
                Some(code),
                if code == 0 {
                    "Task finished successfully".to_string()
                } else {
                    format!("Task failed with exit code {}", code)
                },
            ),
            Err(error) => (false, None, error),
        };

        let _ = handle.emit(
            "bulk_game_task_finished",
            BulkTaskFinished {
                mode,
                success,
                exit_code,
                message,
            },
        );
    });

    Ok(())
}

#[tauri::command(rename_all = "snake_case")]
fn list_generated_games() -> Result<Vec<GeneratedGame>, String> {
    let app_dir = get_app_dir()?;
    let games_dir = app_dir.join("games");
    let game_names = read_generated_game_names(&app_dir);

    if !games_dir.exists() {
        return Ok(Vec::new());
    }

    let mut games = Vec::new();
    let entries = std::fs::read_dir(&games_dir)
        .map_err(|e| format!("Failed to read games directory: {}", e))?;

    for entry in entries {
        let entry = entry.map_err(|e| format!("Failed to read game folder entry: {}", e))?;
        let game_path = entry.path();
        if !game_path.is_dir() {
            continue;
        }

        let app_id = entry.file_name().to_string_lossy().to_string();
        games.push(build_generated_game_details(
            &app_dir,
            &app_id,
            &game_names,
        )?);
    }

    games.sort_by(|a, b| {
        a.name
            .as_deref()
            .unwrap_or(&a.app_id)
            .cmp(b.name.as_deref().unwrap_or(&b.app_id))
    });

    Ok(games)
}

#[tauri::command(rename_all = "snake_case")]
fn list_generated_game_summaries() -> Result<Vec<GeneratedGameSummary>, String> {
    let app_dir = get_app_dir()?;
    let games_dir = app_dir.join("games");
    let game_names = read_generated_game_names(&app_dir);

    if !games_dir.exists() {
        return Ok(Vec::new());
    }

    let mut games = Vec::new();
    let entries = std::fs::read_dir(&games_dir)
        .map_err(|e| format!("Failed to read games directory: {}", e))?;

    for entry in entries {
        let entry = match entry {
            Ok(e) => e,
            Err(_) => continue,
        };
        let game_path = entry.path();
        if !game_path.is_dir() {
            continue;
        }

        let app_id = entry.file_name().to_string_lossy().to_string();
        let modified = entry
            .metadata()
            .ok()
            .and_then(|metadata| metadata.modified().ok())
            .map(system_time_to_unix_seconds)
            .unwrap_or_default();

        games.push(GeneratedGameSummary {
            name: game_names.get(&app_id).cloned(),
            folder_path: game_path.to_string_lossy().to_string(),
            modified,
            app_id,
        });
    }

    games.sort_by(|a, b| {
        a.name
            .as_deref()
            .unwrap_or(&a.app_id)
            .cmp(b.name.as_deref().unwrap_or(&b.app_id))
    });

    Ok(games)
}

#[tauri::command(rename_all = "snake_case")]
fn get_generated_game_details(app_id: String) -> Result<GeneratedGame, String> {
    let app_dir = get_app_dir()?;
    let game_names = read_generated_game_names(&app_dir);
    build_generated_game_details(&app_dir, app_id.trim(), &game_names)
}

#[tauri::command(rename_all = "snake_case")]
fn open_generated_game_folder(app_id: String) -> Result<(), String> {
    let app_dir = get_app_dir()?;
    let folder_path = generated_game_folder(&app_dir, &app_id)?;
    if !folder_path.exists() {
        return Err("Generated game folder does not exist".to_string());
    }

    #[cfg(target_os = "windows")]
    {
        Command::new("explorer")
            .arg(folder_path)
            .spawn()
            .map_err(|e| format!("Failed to open generated game folder: {}", e))?;
    }

    #[cfg(not(target_os = "windows"))]
    {
        Command::new("xdg-open")
            .arg(folder_path)
            .spawn()
            .map_err(|e| format!("Failed to open generated game folder: {}", e))?;
    }

    Ok(())
}

#[tauri::command(rename_all = "snake_case")]
fn delete_generated_game(app_id: String) -> Result<CleanupResult, String> {
    let app_dir = get_app_dir()?;
    let folder_path = generated_game_folder(&app_dir, &app_id)?;
    remove_path(&folder_path)
}

#[tauri::command(rename_all = "snake_case")]
fn sync_new_games(games: Vec<NewGameCatalogItem>) -> Result<NewGamesSyncResponse, String> {
    let now = now_unix_seconds();
    let mut metadata = read_new_games_metadata();
    let normalized_games = games
        .into_iter()
        .filter_map(normalize_game_catalog_item)
        .collect::<Vec<_>>();

    if normalized_games.is_empty() {
        metadata.last_checked_at = now;
        write_new_games_metadata(&metadata)?;
        return Ok(NewGamesSyncResponse {
            state: new_games_state(&metadata),
            new_games: Vec::new(),
        });
    }

    let mut known_ids = metadata
        .known_ids
        .iter()
        .cloned()
        .collect::<HashSet<String>>();
    let first_snapshot = known_ids.is_empty();
    let mut newly_detected = Vec::new();

    for game in &normalized_games {
        if known_ids.insert(game.id.clone()) && !first_snapshot {
            newly_detected.push(NewGameItem {
                id: game.id.clone(),
                name: game.name.clone(),
                icon_hash: game.icon_hash.clone(),
                executable_count: game.executable_count,
                detected_at: now,
            });
        }
    }

    if !newly_detected.is_empty() {
        let new_ids = newly_detected
            .iter()
            .map(|game| game.id.clone())
            .collect::<HashSet<_>>();
        metadata
            .new_games
            .retain(|game| !new_ids.contains(&game.id));

        let mut new_games = newly_detected.clone();
        new_games.extend(std::mem::take(&mut metadata.new_games));
        new_games.truncate(250);
        metadata.new_games = new_games;
    }

    let mut known_ids = known_ids.into_iter().collect::<Vec<_>>();
    known_ids.sort();
    metadata.schema_version = 1;
    metadata.updated_at = now;
    metadata.last_checked_at = now;
    metadata.known_count = known_ids.len();
    metadata.known_ids = known_ids;

    write_new_games_metadata(&metadata)?;

    Ok(NewGamesSyncResponse {
        state: new_games_state(&metadata),
        new_games: newly_detected,
    })
}

#[tauri::command(rename_all = "snake_case")]
fn get_new_games_state() -> Result<NewGamesState, String> {
    Ok(new_games_state(&read_new_games_metadata()))
}

#[tauri::command(rename_all = "snake_case")]
fn clear_new_games() -> Result<NewGamesState, String> {
    let mut metadata = read_new_games_metadata();
    metadata.new_games.clear();
    metadata.updated_at = now_unix_seconds();
    write_new_games_metadata(&metadata)?;
    Ok(new_games_state(&metadata))
}

/// Usage: Calling from JS:
/// ```javascript
/// await invoke('connect_to_discord_rpc_3', json, 'connect' | 'disconnect');
#[tauri::command(rename_all = "snake_case")]
async fn connect_to_discord_rpc_3(
    handle: AppHandle,
    activity_json: String,
    action: String,
) -> Result<(), String> {
    let event_connecting = "client_connecting";
    let event_connected = "client_connected";

    if action.eq_ignore_ascii_case("disconnect") {
        let client_option = {
            let mut client_guard = get_discord_client()
                .lock()
                .map_err(|_| "Discord client state is unavailable".to_string())?;
            client_guard.take()
        };
        if let Some(client) = client_option {
            client.discord.disconnect().await;
        }
        return Ok(());
    }

    let activity = runner::parse_activity_json(&activity_json)?;
    let app_id = activity.app_id.clone();

    let connecting_payload = serde_json::json!({
        "app_id": app_id,
    });

    let previous_client = {
        let mut client_guard = get_discord_client()
            .lock()
            .map_err(|_| "Discord client state is unavailable".to_string())?;
        client_guard.take()
    };

    if let Some(client) = previous_client {
        client.discord.disconnect().await;
    }

    let _ = handle.emit(event_connecting, connecting_payload);

    let client = runner::set_activity(activity.clone()).await?;

    let connected_payload = serde_json::json!({
        "app_id": activity.app_id,
    });

    {
        let mut client_guard = get_discord_client()
            .lock()
            .map_err(|_| "Discord client state is unavailable".to_string())?;
        *client_guard = Some(client);
    }

    let _ = handle.emit(event_connected, connected_payload);

    Ok(())
}

const SEARCH_PRIORITY_CACHE_TTL_SECONDS: u64 = 30 * 24 * 60 * 60; // 30 days

#[tauri::command(rename_all = "snake_case")]
async fn refresh_search_priority(
    target_count: Option<u32>,
) -> Result<SearchPriorityResponse, String> {
    let target_count = normalize_search_priority_target_count(target_count);
    let cached = read_cached_search_priority();
    let now = now_unix_seconds();

    // If cache exists, is less than 30 days old, AND has at least target_count items, use cache immediately
    if let Some(ref data) = cached {
        let is_fresh = now.saturating_sub(data.updated_at) < SEARCH_PRIORITY_CACHE_TTL_SECONDS;
        let is_complete = data.items.len() >= target_count as usize;

        if is_fresh && is_complete {
            return Ok(SearchPriorityResponse {
                data: data.clone(),
                refreshed: false,
                used_cache: true,
                error: None,
            });
        }
    }

    // Otherwise (missing cache, expired cache > 30 days, or incomplete items < target_count), fetch fresh data
    match fetch_search_priority_data(target_count).await {
        Ok(data) => {
            let path = search_priority_path()?;
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent)
                    .map_err(|e| format!("Failed to create metadata folder: {}", e))?;
            }

            let serialized = serde_json::to_vec_pretty(&data)
                .map_err(|e| format!("Failed to serialize search priority: {}", e))?;
            std::fs::write(&path, serialized)
                .map_err(|e| format!("Failed to write search priority: {}", e))?;

            Ok(SearchPriorityResponse {
                data,
                refreshed: true,
                used_cache: false,
                error: None,
            })
        }
        Err(error) => {
            if let Some(data) = cached {
                Ok(SearchPriorityResponse {
                    data,
                    refreshed: false,
                    used_cache: true,
                    error: Some(error),
                })
            } else {
                Ok(SearchPriorityResponse {
                    data: empty_search_priority_data(),
                    refreshed: false,
                    used_cache: false,
                    error: Some(error),
                })
            }
        }
    }
}

#[tauri::command(rename_all = "snake_case")]
async fn fetch_gamelist_gh_mirror() -> Result<String, String> {
    fetch_detectable_games().await
}

#[tauri::command(rename_all = "snake_case")]
async fn fetch_gamelist_from_discord() -> Result<String, String> {
    fetch_detectable_games().await
}

async fn fetch_detectable_games() -> Result<String, String> {
    let client = tauri_plugin_http::reqwest::Client::builder()
        .user_agent(concat!("DiscordQuestHelper/", env!("CARGO_PKG_VERSION")))
        .timeout(Duration::from_secs(30))
        .build()
        .map_err(|e| format!("Failed to create game list client: {}", e))?;
    let response = client
        .get("https://discord.com/api/applications/detectable")
        .send()
        .await
        .map_err(|e| format!("Failed to fetch game list: {}", e))?;
    if !response.status().is_success() {
        return Err(format!(
            "Discord game list returned HTTP {}",
            response.status()
        ));
    }

    response
        .text()
        .await
        .map_err(|e| format!("Failed to read game list response: {}", e))
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    if let Err(error) = tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_http::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .manage(updates::State::new(updates::UpdateStatus::default()))
        .manage(promo_presence::State::new(
            promo_presence::PromoStatus::default(),
        ))
        .setup(|app| {
            if let Ok(app_dir) = get_app_dir() {
                let _ = ensure_embedded_base_files(&app_dir);
            }
            updates::start(app.handle().clone());
            promo_presence::start(app.handle().clone());
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            create_fake_game,
            create_active_quest_games,
            get_game_process_statuses,
            get_running_generated_game_processes,
            stop_process,
            clear_icon_cache,
            clear_generated_games,
            get_bulk_task_availability,
            start_bulk_game_task,
            list_generated_games,
            list_generated_game_summaries,
            get_generated_game_details,
            open_generated_game_folder,
            delete_generated_game,
            sync_new_games,
            get_new_games_state,
            clear_new_games,
            connect_to_discord_rpc_3,
            run_background_process,
            refresh_search_priority,
            fetch_gamelist_gh_mirror,
            fetch_gamelist_from_discord,
            updates::get_update_status,
            promo_presence::set_promo_presence,
            promo_presence::get_promo_presence_status,
        ])
        .run(tauri::generate_context!())
    {
        eprintln!("error while running tauri application: {}", error);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn play_and_stream_quests_are_told_apart() {
        let play = serde_json::json!({
            "task_config_v2": {"tasks": {"PLAY_ON_DESKTOP": {"applications": [{"id": "1"}]}}}
        });
        assert_eq!(quest_task_kinds(&play), (true, false));

        let stream = serde_json::json!({
            "task_config_v2": {"tasks": {"STREAM_ON_DESKTOP": {"applications": [{"id": "1"}]}}}
        });
        assert_eq!(quest_task_kinds(&stream), (false, true));

        let both = serde_json::json!({
            "taskConfig": {"tasks": {"PLAY_ON_DESKTOP": {}, "STREAM_ON_DESKTOP": {}}}
        });
        assert_eq!(quest_task_kinds(&both), (true, true));

        let neither = serde_json::json!({"tasks": {"WATCH_VIDEO": {}}});
        assert_eq!(quest_task_kinds(&neither), (false, false));
    }

    #[test]
    fn a_stream_only_quest_is_not_marked_playable() {
        let item = serde_json::json!({
            "id": "123",
            "config": {
                "id": "123",
                "expires_at": "2030-01-01T00:00:00Z",
                "messages": {"quest_name": "Stream quest"},
                "task_config_v2": {
                    "tasks": {"STREAM_ON_DESKTOP": {"applications": [{"id": "999", "name": "Example"}]}}
                }
            }
        });
        let quest = extract_active_desktop_quest(&item, "Discord Public API", 1_700_000_000)
            .expect("stream quest must be read");
        assert!(!quest.play_task);
        assert!(quest.stream_task);
        assert_eq!(quest.application_ids, vec!["999".to_string()]);
    }

    #[test]
    fn a_quest_with_both_tasks_stays_playable() {
        let item = serde_json::json!({
            "id": "456",
            "config": {
                "id": "456",
                "expires_at": "2030-01-01T00:00:00Z",
                "messages": {"quest_name": "Play and stream quest"},
                "task_config_v2": {
                    "tasks": {
                        "PLAY_ON_DESKTOP": {"applications": [{"id": "777", "name": "Example"}]},
                        "STREAM_ON_DESKTOP": {"applications": [{"id": "777", "name": "Example"}]}
                    }
                }
            }
        });
        let quest = extract_active_desktop_quest(&item, "Discord Public API", 1_700_000_000)
            .expect("quest must be read");
        assert!(quest.play_task);
        assert!(quest.stream_task);
    }

    #[test]
    fn rpc_is_embedded_only_for_the_second_source() {
        let app_id = "1445160676522856458";
        assert_eq!(
            rpc_application_for_source("2.Source/bin", app_id).unwrap(),
            Some(app_id)
        );
        assert_eq!(
            rpc_application_for_source("2.Source\\bin", app_id).unwrap(),
            Some(app_id)
        );
        for path in ["1.Source", "4.Source", "2.OpenAsar", "other/2.Source"] {
            assert_eq!(
                rpc_application_for_source(path, app_id).unwrap(),
                None,
                "{path}"
            );
        }
        for app_id in ["0", "", "-1", "invalid", "18446744073709551615"] {
            assert!(rpc_application_for_source("2.Source", app_id).is_err());
        }
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn generated_runner_resources_carry_rpc_id_only_when_requested() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = env::temp_dir().join(format!("dqh-rpc-resource-{}-{unique}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        for (filename, rpc_id) in [
            ("catalog.exe", None),
            ("appearance.exe", Some("1445160676522856458")),
        ] {
            let file = dir.join(filename);
            std::fs::write(&file, EMBEDDED_RUNNER_EXE).unwrap();
            exe_icon::set_metadata(&file, "Grand Theft Auto VI", rpc_id, None).unwrap();
            assert_eq!(
                exe_icon::read_rpc_id(&file),
                rpc_id.map(|id| id.as_bytes().to_vec())
            );
            std::fs::remove_file(file).unwrap();
        }
        std::fs::remove_dir(dir).unwrap();
    }

    #[test]
    fn icon_validation_rejects_http_error_pages_and_incomplete_png() {
        assert!(!is_valid_icon_png(b"<html>Not Found</html>"));
        assert!(!is_valid_icon_png(b"\x89PNG\r\n\x1a\n"));
        let mut bytes = b"\x89PNG\r\n\x1a\n".to_vec();
        bytes.extend_from_slice(&13u32.to_be_bytes());
        bytes.extend_from_slice(b"IHDR");
        bytes.extend_from_slice(&256u32.to_be_bytes());
        bytes.extend_from_slice(&256u32.to_be_bytes());
        bytes.resize(33, 0);
        assert!(is_valid_icon_png(&bytes));
    }

    #[test]
    fn game_names_are_persisted_and_preserve_existing_entries() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = env::temp_dir().join(format!("dqh-name-test-{}-{unique}", std::process::id()));
        store_generated_game_names(&dir, HashMap::from([("123".into(), "Minecraft".into())]))
            .unwrap();
        store_generated_game_names(&dir, HashMap::from([("456".into(), "Another Game".into())]))
            .unwrap();
        let names = read_generated_game_names(&dir);
        assert_eq!(names.get("123").map(String::as_str), Some("Minecraft"));
        assert_eq!(names.get("456").map(String::as_str), Some("Another Game"));
        std::fs::remove_file(dir.join("metadata/game-names.json")).unwrap();
        std::fs::remove_dir(dir.join("metadata")).unwrap();
        std::fs::remove_dir(dir).unwrap();
    }

    #[test]
    fn rejects_unsafe_windows_path_segments() {
        for value in [
            "..",
            ".",
            "../escape",
            "x/y",
            "x\\y",
            "C:escape",
            "CON",
            "NUL.exe",
            "COM1.exe",
            "trailing.",
        ] {
            assert!(safe_path_segment(value, "test").is_err(), "{value}");
        }
        assert_eq!(
            safe_path_segment("Minecraft.exe", "test").unwrap(),
            "Minecraft.exe"
        );
    }

    #[test]
    fn generated_paths_stay_under_games_and_keep_source_folder() {
        let root = Path::new("C:/test-package");
        assert_eq!(
            game_executable_path(root, "123", "4.Source/bin", "Minecraft.exe").unwrap(),
            root.join("games")
                .join("123")
                .join("4.Source")
                .join("bin")
                .join("Minecraft.exe")
        );
        assert!(game_executable_path(root, "..", "", "game.exe").is_err());
        assert!(game_executable_path(root, "123", "../escape", "game.exe").is_err());
    }

    #[test]
    fn catalog_executables_reject_traversal_and_non_windows_files() {
        for (name, os) in [
            ("../escape.exe", "win32"),
            ("C:/escape.exe", "win32"),
            ("x.bat", "win32"),
            ("x.exe", "linux"),
        ] {
            let item = ActiveQuestGameExecutable {
                name: name.into(),
                os: os.into(),
                requires_risk_acceptance: false,
            };
            assert!(split_catalog_executable(&item).is_none(), "{name} / {os}");
        }
    }

    #[test]
    fn test_multi_game_quest_selects_first_eligible_game_with_executable() {
        let quest = ActiveQuestInfo {
            id: "1544412209935814758".to_string(),
            name: "CAPCOM TGS Deals".to_string(),
            application_ids: vec![
                "1456485028350656512".to_string(), // Resident Evil Requiem (no exe)
                "1448369915462549616".to_string(), // PRAGMATA (has pragmata_sketchbook.exe)
                "1344912009807794256".to_string(), // Monster Hunter Wilds (has monsterhunterwilds.exe)
            ],
            game_names: vec!["CAPCOM TGS Deals".to_string()],
            expires_at: 1800000000,
            play_task: true,
            stream_task: false,
        };

        let games = vec![
            ActiveQuestGameCatalogItem {
                id: "1456485028350656512".to_string(),
                name: "Resident Evil Requiem".to_string(),
                icon_hash: None,
                aliases: Vec::new(),
                executables: Vec::new(), // No executables!
            },
            ActiveQuestGameCatalogItem {
                id: "1448369915462549616".to_string(),
                name: "PRAGMATA".to_string(),
                icon_hash: None,
                aliases: Vec::new(),
                executables: vec![ActiveQuestGameExecutable {
                    name: "pragmata_sketchbook.exe".to_string(),
                    os: "win32".to_string(),
                    requires_risk_acceptance: false,
                }],
            },
            ActiveQuestGameCatalogItem {
                id: "1344912009807794256".to_string(),
                name: "Monster Hunter Wilds".to_string(),
                icon_hash: None,
                aliases: Vec::new(),
                executables: vec![ActiveQuestGameExecutable {
                    name: "monsterhunterwilds/monsterhunterwilds.exe".to_string(),
                    os: "win32".to_string(),
                    requires_risk_acceptance: false,
                }],
            },
        ];

        let (by_id, by_name) = build_active_game_maps(games);
        let matched = match_active_quest_game(&quest, &by_id, &by_name);

        assert!(matched.is_some(), "A game should have been matched");
        let matched = matched.unwrap();
        // Skip earlier candidates that have no Windows executable.
        assert_eq!(matched.game.id, "1448369915462549616");
        assert_eq!(matched.game.name, "PRAGMATA");
        assert!(!matched.fallback_executable_used);
        assert_eq!(matched.score, 1000);
    }

    #[test]
    fn test_multi_game_quest_skips_when_no_games_have_executables() {
        let quest = ActiveQuestInfo {
            id: "1544412209935814758".to_string(),
            name: "CAPCOM TGS Deals".to_string(),
            application_ids: vec![
                "1456485028350656512".to_string(),
                "1448369915462549616".to_string(),
            ],
            game_names: vec!["CAPCOM TGS Deals".to_string()],
            expires_at: 1800000000,
            play_task: true,
            stream_task: false,
        };

        let games = vec![
            ActiveQuestGameCatalogItem {
                id: "1456485028350656512".to_string(),
                name: "Resident Evil Requiem".to_string(),
                icon_hash: None,
                aliases: Vec::new(),
                executables: Vec::new(),
            },
            ActiveQuestGameCatalogItem {
                id: "1448369915462549616".to_string(),
                name: "PRAGMATA".to_string(),
                icon_hash: None,
                aliases: Vec::new(),
                executables: Vec::new(),
            },
        ];

        let (by_id, by_name) = build_active_game_maps(games);
        let matched = match_active_quest_game(&quest, &by_id, &by_name);

        assert!(
            matched.is_none(),
            "Missing catalog executables must not force an App ID activity"
        );
    }

    #[test]
    fn test_collect_quest_application_ids_extracts_all_task_applications() {
        let json = serde_json::json!({
            "application": {
                "id": "1456485028350656512",
                "name": "Resident Evil Requiem"
            },
            "task_config_v2": {
                "tasks": {
                    "PLAY_ON_DESKTOP": {
                        "type": "PLAY_ON_DESKTOP",
                        "applications": [
                            { "id": "1456485028350656512" },
                            { "id": "1448369915462549616" },
                            { "id": "1344912009807794256" }
                        ]
                    }
                }
            }
        });

        let ids = collect_quest_application_ids(&json);
        assert!(ids.contains(&"1456485028350656512".to_string()));
        assert!(ids.contains(&"1448369915462549616".to_string()));
        assert!(ids.contains(&"1344912009807794256".to_string()));
        assert_eq!(ids.len(), 3);
    }

    #[test]
    fn test_quest_without_executable_does_not_match_unrelated_game() {
        let quest = ActiveQuestInfo {
            id: "1552793125028634754".to_string(),
            name: "EA SPORTS FC™ 27".to_string(),
            application_ids: vec!["1531874756096295054".to_string()],
            game_names: vec!["EA SPORTS FC™ 27".to_string()],
            expires_at: 1800000000,
            play_task: true,
            stream_task: false,
        };

        let games = vec![
            ActiveQuestGameCatalogItem {
                id: "1531874756096295054".to_string(),
                name: "EA SPORTS FC™ 27".to_string(),
                icon_hash: None,
                aliases: Vec::new(),
                executables: Vec::new(), // EA Sports FC has no executables in detectable list
            },
            ActiveQuestGameCatalogItem {
                id: "1442716382067425290".to_string(),
                name: "34 Sports Games: World Edition".to_string(),
                icon_hash: None,
                aliases: Vec::new(),
                executables: vec![ActiveQuestGameExecutable {
                    name: "34-sports-games-world-edition.exe".to_string(),
                    os: "win32".to_string(),
                    requires_risk_acceptance: false,
                }],
            },
        ];

        let (by_id, by_name) = build_active_game_maps(games);
        let matched = match_active_quest_game(&quest, &by_id, &by_name);

        assert!(
            matched.is_none(),
            "Do not substitute an unrelated game or force App ID RPC"
        );
    }

    #[test]
    fn test_upcoming_quest_within_7_days_accepted() {
        let now = 1790000000;
        let starts_at = now + 3 * 86400; // 3 days in the future
        let expires_at = now + 10 * 86400; // 10 days in the future

        let item = serde_json::json!({
            "id": "1550563498394787951",
            "config": {
                "id": "1550563498394787951",
                "starts_at": time::OffsetDateTime::from_unix_timestamp(starts_at as i64).unwrap().format(&time::format_description::well_known::Rfc3339).unwrap(),
                "expires_at": time::OffsetDateTime::from_unix_timestamp(expires_at as i64).unwrap().format(&time::format_description::well_known::Rfc3339).unwrap(),
                "messages": {
                    "quest_name": "NTE Half Anni"
                },
                "task_config_v2": {
                    "tasks": {
                        "PLAY_ON_DESKTOP": {
                            "type": "PLAY_ON_DESKTOP",
                            "applications": [
                                { "id": "1470616226995765409" }
                            ]
                        }
                    }
                }
            }
        });

        let extracted = extract_active_desktop_quest(&item, "Public API", now);
        assert!(
            extracted.is_some(),
            "Upcoming quest within 7 days should be extracted"
        );
        let quest = extracted.unwrap();
        assert_eq!(quest.id, "1550563498394787951");
        assert_eq!(quest.application_ids, vec!["1470616226995765409"]);
    }

    #[tokio::test]
    #[ignore = "manual live API smoke test; quest catalog changes over time"]
    async fn test_fetch_active_quests_live() {
        let (quests, sources, errors) = fetch_active_desktop_quests().await.unwrap();
        println!("Used sources: {:?}", sources);
        println!("Errors: {:?}", errors);
        println!("Total quests found: {}", quests.len());
        for q in &quests {
            println!(
                "Quest: {} | Name: {} | Apps: {:?} | Names: {:?}",
                q.id, q.name, q.application_ids, q.game_names
            );
        }
        let found = quests.iter().find(|q| q.id == "1549171029241233449");
        assert!(found.is_some(), "Quest 1549171029241233449 must be found");
        let apex_quest = found.unwrap();
        let games = vec![ActiveQuestGameCatalogItem {
            id: "542075586886107149".to_string(),
            name: "Apex Legends".to_string(),
            icon_hash: None,
            aliases: vec!["Apex Legends".to_string()],
            executables: vec![ActiveQuestGameExecutable {
                name: "r5apex.exe".to_string(),
                os: "win32".to_string(),
                requires_risk_acceptance: false,
            }],
        }];
        let (by_id, by_name) = build_active_game_maps(games);
        let matched = match_active_quest_game(apex_quest, &by_id, &by_name);
        assert!(matched.is_some(), "Apex Legends must be matched");
        let matched = matched.unwrap();
        assert_eq!(matched.game.id, "542075586886107149");
        assert_eq!(matched.method, "app_id");
    }
}
