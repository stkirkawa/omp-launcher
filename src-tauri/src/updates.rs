use base64::{engine::general_purpose::STANDARD, Engine};
use log::{info, warn};
use reqwest::{redirect::Policy, Client, Response, StatusCode};
use ring::signature::{UnparsedPublicKey, ED25519};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::HashSet,
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Component, Path, PathBuf},
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use tauri::Window;

const MANIFEST_URL: &str = "http://2.27.98.254/kirkawa/manifest.json";
const SIGNATURE_URL: &str = "http://2.27.98.254/kirkawa/manifest.sig";
const FILES_URL: &str = "http://2.27.98.254/kirkawa/files/";
const GAME_PACKAGE_URL: &str = "http://2.27.98.254/kirkawa/Kirkawa-Game.7z";
const RELEASE_NOTES_URL: &str = "http://2.27.98.254/kirkawa/releases/";
const MAX_MANIFEST_BYTES: usize = 2 * 1024 * 1024;
const MAX_SIGNATURE_BYTES: usize = 256;
const MAX_RELEASE_NOTES_BYTES: usize = 64 * 1024;
const MAX_RELEASE_NOTES_CHARS: usize = 12_000;
const MAX_MANAGED_FILES: usize = 512;
const MAX_MANAGED_FILE_BYTES: u64 = 512 * 1024 * 1024;
const MAX_MANAGED_TOTAL_BYTES: u64 = 2 * 1024 * 1024 * 1024;
const MAX_GAME_ARCHIVE_BYTES: u64 = 12 * 1024 * 1024 * 1024;
const MAX_GAME_UNPACKED_BYTES: u64 = 16 * 1024 * 1024 * 1024;
const MAX_GAME_ARCHIVE_ENTRIES: usize = 4096;
const DOWNLOAD_CHUNK_BYTES: u64 = 1024 * 1024;
const MAX_ARCHIVE_ENTRY_BYTES: u64 = 2 * 1024 * 1024 * 1024;
const SIGNING_PUBLIC_KEY: [u8; 32] = [
    0xae, 0x35, 0x53, 0xc7, 0xdb, 0x22, 0x79, 0x41, 0xe8, 0x12, 0xd5, 0xb7, 0xdf, 0x2d, 0x66, 0xcb,
    0xc7, 0x71, 0x61, 0xae, 0x65, 0x19, 0x05, 0xba, 0xc8, 0xea, 0xb3, 0x5b, 0x42, 0x65, 0x1f, 0xc5,
];

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct BuildManifest {
    version: String,
    game_package: GamePackage,
    files: Vec<ManagedFile>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct GamePackage {
    size: u64,
    unpacked_size: u64,
    sha256: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ManagedFile {
    path: String,
    size: u64,
    sha256: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct LocalManifestState {
    manifest: String,
    signature: String,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ReleaseNotes {
    version: String,
    changelog: String,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateInfo {
    version: String,
    current_version: Option<String>,
    changelog: String,
    update_available: bool,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct UpdateProgress {
    phase: String,
    message: String,
    version: String,
    current_file: String,
    percent: u8,
    downloaded_bytes: u64,
    total_bytes: u64,
    completed_files: u32,
    total_files: u32,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateResult {
    version: String,
    updated_files: u32,
    downloaded_bytes: u64,
    installed_build: bool,
}

#[tauri::command]
pub async fn check_for_updates() -> std::result::Result<UpdateInfo, String> {
    let client = Client::builder()
        .connect_timeout(Duration::from_secs(20))
        .timeout(Duration::from_secs(60))
        .read_timeout(Duration::from_secs(30))
        .redirect(Policy::none())
        .user_agent("KirkawaLauncher/1.0")
        .build()
        .map_err(|error| format!("Не удалось настроить проверку обновлений: {error}"))?;

    let (manifest, _, _) = fetch_manifest(&client).await?;
    let changelog = fetch_release_notes(&client, &manifest.version).await?;
    let current_version = installed_manifest_version()?;
    let update_available = current_version.as_deref() != Some(&manifest.version);

    Ok(UpdateInfo {
        version: manifest.version,
        current_version,
        changelog,
        update_available,
    })
}

#[tauri::command]
pub async fn install_or_update(
    window: Window,
    game_path: String,
) -> std::result::Result<UpdateResult, String> {
    let requested_root = PathBuf::from(game_path.trim());
    if !requested_root.is_absolute() || !requested_root.is_dir() {
        return Err("Выберите существующую папку для установки игры.".to_string());
    }

    let client = Client::builder()
        .connect_timeout(Duration::from_secs(20))
        .timeout(Duration::from_secs(2 * 60 * 60))
        .read_timeout(Duration::from_secs(90))
        .redirect(Policy::none())
        .user_agent("KirkawaLauncher/1.0")
        .build()
        .map_err(|error| format!("Не удалось настроить загрузчик: {error}"))?;

    let (manifest, manifest_bytes, signature) = fetch_manifest(&client).await?;
    let requested_root = requested_root
        .canonicalize()
        .map_err(|error| format!("Не удалось проверить выбранную папку игры: {error}"))?;

    match game_installation_status(&requested_root)? {
        true => {
            let (updated_files, downloaded_bytes) =
                update_managed_files(&window, &client, &requested_root, &manifest).await?;
            save_manifest_state(&manifest_bytes, &signature)?;
            emit_progress(
                &window,
                progress(
                    "complete",
                    "Обновление файлов завершено.",
                    &manifest.version,
                    "",
                    100,
                    downloaded_bytes,
                    downloaded_bytes,
                    updated_files,
                    updated_files,
                ),
            )?;

            Ok(UpdateResult {
                version: manifest.version,
                updated_files,
                downloaded_bytes,
                installed_build: false,
            })
        }
        false => {
            ensure_directory_is_empty(&requested_root)?;
            let (updated_files, downloaded_bytes) = install_game_package(
                &window,
                &client,
                &requested_root,
                &manifest,
                &manifest_bytes,
                &signature,
            )
            .await?;

            Ok(UpdateResult {
                version: manifest.version,
                updated_files,
                downloaded_bytes,
                installed_build: true,
            })
        }
    }
}

fn game_installation_status(root: &Path) -> std::result::Result<bool, String> {
    let executable = root.join("gta_sa.exe");
    let samp = root.join("samp.dll");
    let has_executable = executable.is_file();
    let has_samp = samp.is_file();

    match (has_executable, has_samp) {
        (true, true) => Ok(true),
        (false, false) => Ok(false),
        _ => Err(
            "В папке обнаружена неполная установка. Укажите исправную GTA: San Andreas с SA-MP или пустую папку."
                .to_string(),
        ),
    }
}

fn ensure_directory_is_empty(path: &Path) -> std::result::Result<(), String> {
    let mut entries = fs::read_dir(path)
        .map_err(|error| format!("Не удалось проверить папку установки: {error}"))?;

    if entries
        .next()
        .transpose()
        .map_err(|error| format!("Не удалось прочитать папку установки: {error}"))?
        .is_some()
    {
        return Err(
            "Для новой сборки выберите пустую папку. Существующие файлы игры и модификации не изменены."
                .to_string(),
        );
    }

    Ok(())
}

async fn fetch_manifest(
    client: &Client,
) -> std::result::Result<(BuildManifest, Vec<u8>, String), String> {
    let manifest_response = get_checked_response(client, MANIFEST_URL).await?;
    let manifest_bytes =
        read_limited_response(manifest_response, MAX_MANIFEST_BYTES, "манифест").await?;
    if manifest_bytes.is_empty() {
        return Err("Размер манифеста сборки недопустим.".to_string());
    }

    let signature_response = get_checked_response(client, SIGNATURE_URL).await?;
    let signature_bytes =
        read_limited_response(signature_response, MAX_SIGNATURE_BYTES, "подпись").await?;
    let signature = std::str::from_utf8(&signature_bytes)
        .map_err(|_| "Подпись манифеста не является корректным текстом.".to_string())?
        .trim()
        .to_string();

    let manifest_text = std::str::from_utf8(&manifest_bytes)
        .map_err(|_| "Манифест сборки не является корректным UTF-8.".to_string())?;
    verify_manifest_signature(manifest_text.as_bytes(), &signature, &SIGNING_PUBLIC_KEY)?;

    let manifest: BuildManifest = serde_json::from_slice(&manifest_bytes)
        .map_err(|error| format!("Не удалось прочитать манифест сборки: {error}"))?;
    validate_manifest(&manifest)?;

    Ok((manifest, manifest_bytes.to_vec(), signature))
}

async fn get_checked_response(client: &Client, url: &str) -> std::result::Result<Response, String> {
    let response = client
        .get(url)
        .send()
        .await
        .map_err(|error| format!("Не удалось подключиться к серверу обновлений: {error}"))?;
    if response.status() != StatusCode::OK {
        return Err(format!(
            "Сервер обновлений вернул HTTP {} для {}.",
            response.status(),
            url
        ));
    }
    Ok(response)
}

async fn read_limited_response(
    mut response: Response,
    maximum_bytes: usize,
    description: &str,
) -> std::result::Result<Vec<u8>, String> {
    if response
        .content_length()
        .map_or(false, |length| length > maximum_bytes as u64)
    {
        return Err(format!(
            "Размер ответа «{description}» превышает допустимый предел."
        ));
    }

    let mut bytes = Vec::new();
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|error| format!("Не удалось получить «{description}»: {error}"))?
    {
        let new_size = bytes
            .len()
            .checked_add(chunk.len())
            .ok_or_else(|| format!("Размер ответа «{description}» переполнен."))?;
        if new_size > maximum_bytes {
            return Err(format!(
                "Размер ответа «{description}» превышает допустимый предел."
            ));
        }
        bytes.extend_from_slice(&chunk);
    }
    Ok(bytes)
}

fn verify_manifest_signature(
    bytes: &[u8],
    encoded_signature: &str,
    public_key: &[u8; 32],
) -> std::result::Result<(), String> {
    let signature_bytes = STANDARD
        .decode(encoded_signature)
        .map_err(|_| "Подпись манифеста повреждена.".to_string())?;
    UnparsedPublicKey::new(&ED25519, public_key)
        .verify(bytes, &signature_bytes)
        .map_err(|_| "Подпись манифеста не прошла проверку.".to_string())
}

async fn fetch_release_notes(
    client: &Client,
    version: &str,
) -> std::result::Result<String, String> {
    let release_url = format!("{RELEASE_NOTES_URL}{version}/release.json");
    let response = client
        .get(&release_url)
        .send()
        .await
        .map_err(|error| format!("Не удалось получить список изменений: {error}"))?;
    if response.status() == StatusCode::NOT_FOUND {
        return Ok("Список изменений для этой версии пока не опубликован.".to_string());
    }
    if response.status() != StatusCode::OK {
        return Err(format!(
            "Сервер обновлений вернул HTTP {} для списка изменений.",
            response.status()
        ));
    }
    let release_bytes =
        read_limited_response(response, MAX_RELEASE_NOTES_BYTES, "список изменений").await?;

    let signature_url = format!("{RELEASE_NOTES_URL}{version}/release.sig");
    let signature_response = get_checked_response(client, &signature_url).await?;
    let signature_bytes = read_limited_response(
        signature_response,
        MAX_SIGNATURE_BYTES,
        "подпись списка изменений",
    )
    .await?;
    let signature = std::str::from_utf8(&signature_bytes)
        .map_err(|_| "Подпись списка изменений повреждена.".to_string())?
        .trim();
    verify_manifest_signature(&release_bytes, signature, &SIGNING_PUBLIC_KEY)?;

    let release: ReleaseNotes = serde_json::from_slice(&release_bytes)
        .map_err(|error| format!("Не удалось прочитать список изменений: {error}"))?;
    if release.version != version {
        return Err("Версия списка изменений не совпадает с версией сборки.".to_string());
    }
    validate_release_notes(&release.changelog)?;
    Ok(release.changelog)
}

fn validate_release_notes(changelog: &str) -> std::result::Result<(), String> {
    if changelog.trim().is_empty()
        || changelog.chars().count() > MAX_RELEASE_NOTES_CHARS
        || changelog
            .chars()
            .any(|character| character.is_control() && !matches!(character, '\n' | '\r' | '\t'))
    {
        return Err("Список изменений имеет недопустимый формат.".to_string());
    }
    Ok(())
}

fn installed_manifest_version() -> std::result::Result<Option<String>, String> {
    let path = manifest_state_path()?;
    let state_bytes = match fs::read(&path) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => {
            return Err(format!(
                "Не удалось проверить установленную версию игры: {error}"
            ))
        }
    };

    let state: LocalManifestState = match serde_json::from_slice(&state_bytes) {
        Ok(state) => state,
        Err(error) => {
            warn!("Ignoring invalid saved launcher manifest state: {}", error);
            return Ok(None);
        }
    };
    if let Err(error) = verify_manifest_signature(
        state.manifest.as_bytes(),
        &state.signature,
        &SIGNING_PUBLIC_KEY,
    ) {
        warn!(
            "Treating the installed version as unknown because its saved manifest is not trusted: {}",
            error
        );
        return Ok(None);
    }

    let manifest: BuildManifest = match serde_json::from_str(&state.manifest) {
        Ok(manifest) => manifest,
        Err(error) => {
            warn!("Ignoring unparsable saved launcher manifest: {}", error);
            return Ok(None);
        }
    };
    if let Err(error) = validate_manifest(&manifest) {
        warn!("Ignoring invalid saved launcher manifest: {}", error);
        return Ok(None);
    }
    Ok(Some(manifest.version))
}

fn validate_manifest(manifest: &BuildManifest) -> std::result::Result<(), String> {
    if manifest.version.is_empty()
        || manifest.version.len() > 64
        || manifest.version.split('.').count() < 2
        || manifest.version.split('.').count() > 4
        || manifest
            .version
            .split('.')
            .any(|part| part.is_empty() || !part.bytes().all(|byte| byte.is_ascii_digit()))
    {
        return Err("Версия сборки в манифесте имеет неверный формат.".to_string());
    }

    if manifest.files.is_empty() || manifest.files.len() > MAX_MANAGED_FILES {
        return Err("Количество файлов сборки в манифесте недопустимо.".to_string());
    }

    let mut seen_paths = HashSet::new();
    let mut total_size = 0u64;
    for file in &manifest.files {
        validate_managed_path(&file.path)?;
        validate_sha256(&file.sha256)?;
        if file.size > MAX_MANAGED_FILE_BYTES {
            return Err(format!(
                "Размер управляемого файла {} превышает допустимый предел.",
                file.path
            ));
        }
        if !seen_paths.insert(file.path.to_ascii_lowercase()) {
            return Err(format!("Манифест содержит повторный путь {}.", file.path));
        }
        total_size = total_size
            .checked_add(file.size)
            .ok_or_else(|| "Суммарный размер файлов сборки переполнен.".to_string())?;
    }
    if total_size > MAX_MANAGED_TOTAL_BYTES {
        return Err("Общий размер обновляемых файлов превышает допустимый предел.".to_string());
    }

    validate_sha256(&manifest.game_package.sha256)?;
    if manifest.game_package.size == 0
        || manifest.game_package.size > MAX_GAME_ARCHIVE_BYTES
        || manifest.game_package.unpacked_size == 0
        || manifest.game_package.unpacked_size > MAX_GAME_UNPACKED_BYTES
    {
        return Err("Параметры архива сборки превышают допустимый предел.".to_string());
    }

    Ok(())
}

fn validate_managed_path(path: &str) -> std::result::Result<(), String> {
    if path.len() > 240 || !path.starts_with("SAMP/") || path.contains('\\') || path.contains(':') {
        return Err(format!("Путь управляемого файла недопустим: {path}"));
    }

    let parts: Vec<_> = path.split('/').collect();
    if parts.len() < 2
        || parts[0] != "SAMP"
        || parts[1..].iter().any(|part| {
            part.is_empty()
                || *part == "."
                || *part == ".."
                || part.ends_with('.')
                || part.ends_with(' ')
                || part
                    .chars()
                    .any(|character| character.is_control() || "<>:\"|?*".contains(character))
                || is_windows_device_name(part)
        })
    {
        return Err(format!("Путь управляемого файла недопустим: {path}"));
    }

    Ok(())
}

fn is_windows_device_name(component: &str) -> bool {
    let stem = component
        .split('.')
        .next()
        .unwrap_or_default()
        .to_ascii_uppercase();
    matches!(stem.as_str(), "CON" | "PRN" | "AUX" | "NUL")
        || stem.strip_prefix("COM").map_or(false, |suffix| {
            matches!(suffix, "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9")
        })
        || stem.strip_prefix("LPT").map_or(false, |suffix| {
            matches!(suffix, "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9")
        })
}

fn validate_archive_path(path: &str) -> std::result::Result<PathBuf, String> {
    if path.is_empty() || path.starts_with('/') || path.starts_with('\\') || path.contains(':') {
        return Err(format!("Архив содержит небезопасный путь: {path}"));
    }

    let normalized = path.replace('\\', "/");
    let mut safe_path = PathBuf::new();
    for part in normalized.split('/') {
        if part.is_empty()
            || part == "."
            || part == ".."
            || part.ends_with('.')
            || part.ends_with(' ')
            || part
                .chars()
                .any(|character| character.is_control() || "<>:\"|?*".contains(character))
            || is_windows_device_name(part)
        {
            return Err(format!("Архив содержит небезопасный путь: {path}"));
        }
        safe_path.push(part);
    }
    if safe_path
        .components()
        .any(|part| !matches!(part, Component::Normal(_)))
    {
        return Err(format!("Архив содержит небезопасный путь: {path}"));
    }

    Ok(safe_path)
}

fn validate_sha256(checksum: &str) -> std::result::Result<(), String> {
    if checksum.len() != 64 || !checksum.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err("Контрольная сумма SHA-256 имеет неверный формат.".to_string());
    }
    Ok(())
}

async fn install_game_package(
    window: &Window,
    client: &Client,
    root: &Path,
    manifest: &BuildManifest,
    manifest_bytes: &[u8],
    signature: &str,
) -> std::result::Result<(u32, u64), String> {
    let parent = root.parent().ok_or_else(|| {
        "Для выбранной папки установки нельзя определить родительский каталог.".to_string()
    })?;
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| format!("Не удалось получить время системы: {error}"))?
        .as_nanos();
    let staging = parent.join(format!(".kirkawa-install-{}-{nonce}", std::process::id()));
    if staging.exists() {
        return Err("Временная папка установки уже существует.".to_string());
    }
    fs::create_dir(&staging)
        .map_err(|error| format!("Не удалось создать временную папку установки: {error}"))?;

    let archive =
        staging.with_file_name(format!(".kirkawa-build-{}-{nonce}.7z", std::process::id()));
    let operation = async {
        emit_progress(
            window,
            progress(
                "checking",
                "Проверяю подписанную версию сборки.",
                &manifest.version,
                "",
                0,
                0,
                manifest.game_package.size,
                0,
                0,
            ),
        )?;
        download_game_archive(window, client, &manifest.game_package, &archive, manifest).await?;

        emit_progress(
            window,
            progress(
                "extracting",
                "Распаковываю сборку. Архив пока не знает, где твой WinRAR.",
                &manifest.version,
                "Kirkawa-Game.7z",
                50,
                manifest.game_package.size,
                manifest.game_package.size,
                0,
                0,
            ),
        )?;
        extract_game_archive(window, &archive, &staging, &manifest.game_package, manifest)?;

        if !game_installation_status(&staging)? {
            return Err("В архиве отсутствует GTA: San Andreas или файл SA-MP.".to_string());
        }

        let (updated_files, file_bytes) =
            update_managed_files(window, client, &staging, manifest).await?;
        save_manifest_state(manifest_bytes, signature)?;

        ensure_directory_is_empty(root)?;
        fs::remove_dir(root).map_err(|error| {
            format!("Не удалось освободить пустую папку установки: {error}")
        })?;
        if let Err(error) = fs::rename(&staging, root) {
            let restore_result = fs::create_dir(root);
            if let Err(restore_error) = restore_result {
                return Err(format!(
                    "Не удалось завершить установку ({error}) и восстановить пустую папку ({restore_error})."
                ));
            }
            return Err(format!("Не удалось разместить установленную сборку: {error}"));
        }

        if let Err(error) = fs::remove_file(&archive) {
            warn!(
                "Installed build, but could not remove temporary archive '{}': {}",
                archive.display(),
                error
            );
        }

        emit_progress(
            window,
            progress(
                "complete",
                "Сборка установлена. Теперь можно заходить на сервер.",
                &manifest.version,
                "",
                100,
                manifest.game_package.size,
                manifest.game_package.size,
                updated_files,
                updated_files,
            ),
        )?;

        let downloaded_bytes = manifest
            .game_package
            .size
            .checked_add(file_bytes)
            .ok_or_else(|| "Размер загрузки сборки переполнен.".to_string())?;
        Ok((updated_files, downloaded_bytes))
    }
    .await;

    if operation.is_err() {
        if let Err(error) = fs::remove_file(&archive) {
            if error.kind() != std::io::ErrorKind::NotFound {
                warn!(
                    "Could not remove incomplete game archive '{}': {}",
                    archive.display(),
                    error
                );
            }
        }
        if let Err(error) = fs::remove_dir_all(&staging) {
            if error.kind() != std::io::ErrorKind::NotFound {
                warn!(
                    "Could not remove incomplete game staging folder '{}': {}",
                    staging.display(),
                    error
                );
            }
        }
    }

    operation
}

async fn download_game_archive(
    window: &Window,
    client: &Client,
    package: &GamePackage,
    destination: &Path,
    manifest: &BuildManifest,
) -> std::result::Result<(), String> {
    let mut response = get_checked_response(client, GAME_PACKAGE_URL).await?;
    if response
        .content_length()
        .map_or(false, |length| length != package.size)
    {
        return Err("Размер архива сборки не совпадает с подписанным манифестом.".to_string());
    }

    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(destination)
        .map_err(|error| format!("Не удалось создать временный архив: {error}"))?;
    let mut hasher = Sha256::new();
    let mut downloaded = 0u64;
    let mut last_reported = 0u64;

    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|error| format!("Не удалось загрузить архив сборки: {error}"))?
    {
        let chunk_size = u64::try_from(chunk.len())
            .map_err(|_| "Размер блока архива не помещается в число.".to_string())?;
        downloaded = downloaded
            .checked_add(chunk_size)
            .ok_or_else(|| "Размер скачанного архива переполнен.".to_string())?;
        if downloaded > package.size {
            return Err("Сервер передал архив больше подписанного размера.".to_string());
        }
        file.write_all(&chunk)
            .map_err(|error| format!("Не удалось сохранить архив сборки: {error}"))?;
        hasher.update(&chunk);

        if downloaded.saturating_sub(last_reported) >= DOWNLOAD_CHUNK_BYTES {
            emit_progress(
                window,
                progress(
                    "downloading",
                    "Скачиваю сборку. Интернет есть, стабильности пока нет.",
                    &manifest.version,
                    "Kirkawa-Game.7z",
                    percent_between(downloaded, package.size, 0, 50),
                    downloaded,
                    package.size,
                    0,
                    0,
                ),
            )?;
            last_reported = downloaded;
        }
    }

    file.sync_all()
        .map_err(|error| format!("Не удалось сохранить загруженный архив: {error}"))?;
    if downloaded != package.size {
        return Err(format!(
            "Архив загружен не полностью: получено {} из {} байт.",
            downloaded, package.size
        ));
    }
    let actual_hash = hex_lower(&hasher.finalize());
    if !actual_hash.eq_ignore_ascii_case(&package.sha256) {
        return Err("Архив сборки не прошёл проверку SHA-256.".to_string());
    }

    emit_progress(
        window,
        progress(
            "downloading",
            "Архив проверен. Теперь самое интересное — распаковка.",
            &manifest.version,
            "Kirkawa-Game.7z",
            50,
            package.size,
            package.size,
            0,
            0,
        ),
    )?;
    Ok(())
}

fn extract_game_archive(
    window: &Window,
    archive: &Path,
    destination: &Path,
    package: &GamePackage,
    manifest: &BuildManifest,
) -> std::result::Result<(), String> {
    use sevenz_rust::decompress_file_with_extract_fn;

    let mut entries = 0usize;
    let mut unpacked = 0u64;
    let mut last_reported = 0u64;
    let mut seen_files = HashSet::new();
    let staging_root = destination
        .canonicalize()
        .map_err(|error| format!("Не удалось проверить временную папку: {error}"))?;

    decompress_file_with_extract_fn(archive, destination, |entry, reader, _library_path| {
        if entry.is_anti_item() || entry.windows_attributes() & 0x400 != 0 {
            return Err(sevenz_rust::Error::other(
                "Архив содержит неподдерживаемую ссылку или анти-файл.",
            ));
        }
        entries += 1;
        if entries > MAX_GAME_ARCHIVE_ENTRIES {
            return Err(sevenz_rust::Error::other(
                "Архив содержит слишком много файлов.",
            ));
        }

        let relative = validate_archive_path(entry.name()).map_err(sevenz_rust::Error::other)?;
        let file_size = entry.size();
        if file_size > MAX_ARCHIVE_ENTRY_BYTES {
            return Err(sevenz_rust::Error::other(
                "Файл в архиве превышает допустимый размер.",
            ));
        }
        let path_key = relative.to_string_lossy().to_ascii_lowercase();
        if !seen_files.insert(path_key) {
            return Err(sevenz_rust::Error::other(
                "Архив содержит повторные пути файлов.",
            ));
        }
        unpacked = unpacked.checked_add(file_size).ok_or_else(|| {
            sevenz_rust::Error::other("Общий размер распакованной сборки переполнен.")
        })?;
        if unpacked > package.unpacked_size || unpacked > MAX_GAME_UNPACKED_BYTES {
            return Err(sevenz_rust::Error::other(
                "Размер файлов в архиве превышает подписанный предел.",
            ));
        }

        let output = staging_root.join(&relative);
        if !output.starts_with(&staging_root) {
            return Err(sevenz_rust::Error::other(
                "Путь файла выходит за папку установки.",
            ));
        }
        if entry.is_directory() {
            fs::create_dir_all(&output).map_err(sevenz_rust::Error::io)?;
            return Ok(true);
        }
        if !entry.has_stream() || output.exists() {
            return Err(sevenz_rust::Error::other(
                "Архив содержит некорректный или повторный файл.",
            ));
        }

        let parent = output
            .parent()
            .ok_or_else(|| sevenz_rust::Error::other("У файла из архива отсутствует папка."))?;
        fs::create_dir_all(parent).map_err(sevenz_rust::Error::io)?;
        let mut output_file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&output)
            .map_err(sevenz_rust::Error::io)?;
        let mut buffer = [0u8; 64 * 1024];
        let mut written = 0u64;

        loop {
            let count = reader.read(&mut buffer).map_err(sevenz_rust::Error::io)?;
            if count == 0 {
                break;
            }
            written = written
                .checked_add(count as u64)
                .ok_or_else(|| sevenz_rust::Error::other("Размер файла переполнен."))?;
            if written > file_size || written > MAX_ARCHIVE_ENTRY_BYTES {
                return Err(sevenz_rust::Error::other(
                    "Распакованный файл превышает размер из архива.",
                ));
            }
            output_file
                .write_all(&buffer[..count])
                .map_err(sevenz_rust::Error::io)?;
            if written.saturating_sub(last_reported) >= 8 * 1024 * 1024 {
                let percent = 50
                    + percent_between(
                        unpacked.saturating_sub(file_size) + written,
                        package.unpacked_size,
                        0,
                        45,
                    );
                emit_progress(
                    window,
                    progress(
                        "extracting",
                        "Распаковываю файлы. Если загрузка зависла, компьютер задумался о жизни.",
                        &manifest.version,
                        entry.name(),
                        percent,
                        package.size,
                        package.size,
                        0,
                        0,
                    ),
                )
                .map_err(sevenz_rust::Error::other)?;
                last_reported = written;
            }
        }
        if written != file_size {
            return Err(sevenz_rust::Error::other(
                "Файл из архива распакован не полностью.",
            ));
        }
        output_file.sync_all().map_err(sevenz_rust::Error::io)?;
        Ok(true)
    })
    .map_err(|error| format!("Не удалось безопасно распаковать сборку: {error}"))?;

    if unpacked != package.unpacked_size {
        return Err(format!(
            "Распаковано {} из {} байт согласно манифесту.",
            unpacked, package.unpacked_size
        ));
    }

    emit_progress(
        window,
        progress(
            "extracting",
            "Файлы распакованы. Проверяю сборку перед установкой.",
            &manifest.version,
            "",
            95,
            package.size,
            package.size,
            0,
            0,
        ),
    )?;
    Ok(())
}

async fn update_managed_files(
    window: &Window,
    client: &Client,
    root: &Path,
    manifest: &BuildManifest,
) -> std::result::Result<(u32, u64), String> {
    let root = root
        .canonicalize()
        .map_err(|error| format!("Не удалось проверить папку игры: {error}"))?;
    if !game_installation_status(&root)? {
        return Err("Для обновления требуется папка с gta_sa.exe и samp.dll.".to_string());
    }

    let mut changed = Vec::new();
    for file in &manifest.files {
        ensure_safe_target(&root, &file.path, false)?;
        let target = root.join(&file.path);
        let current_hash = match fs::metadata(&target) {
            Ok(metadata) if metadata.is_file() => hash_file(&target)?,
            Ok(_) => {
                return Err(format!(
                    "Путь управляемого файла занят папкой: {}",
                    file.path
                ))
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => String::new(),
            Err(error) => {
                return Err(format!(
                    "Не удалось проверить управляемый файл {}: {error}",
                    file.path
                ))
            }
        };
        if !current_hash.eq_ignore_ascii_case(&file.sha256) {
            changed.push(file);
        }
    }

    let total_bytes = changed.iter().try_fold(0u64, |total, file| {
        total
            .checked_add(file.size)
            .ok_or_else(|| "Общий размер обновления переполнен.".to_string())
    })?;
    emit_progress(
        window,
        progress(
            "checking",
            if changed.is_empty() {
                "Проверка завершена. Все управляемые файлы уже актуальны."
            } else {
                "Файлы проверены. Загружаю только отличающиеся."
            },
            &manifest.version,
            "",
            if changed.is_empty() { 99 } else { 0 },
            0,
            total_bytes,
            0,
            changed.len() as u32,
        ),
    )?;

    let mut downloaded = 0u64;
    for (index, file) in changed.iter().enumerate() {
        emit_progress(
            window,
            progress(
                "downloading",
                "Загружаю файл сборки. Проверяем только то, чем управляет лаунчер.",
                &manifest.version,
                &file.path,
                percent_between(
                    downloaded,
                    total_bytes,
                    0,
                    if changed.is_empty() { 100 } else { 95 },
                ),
                downloaded,
                total_bytes,
                index as u32,
                changed.len() as u32,
            ),
        )?;

        let target = root.join(&file.path);
        ensure_safe_target(&root, &file.path, true)?;
        let staged = unique_sibling_path(&target, "kirkawa-new")?;
        let download_result = download_managed_file(
            client,
            file,
            &staged,
            window,
            manifest,
            downloaded,
            total_bytes,
            index as u32,
            changed.len() as u32,
        )
        .await;
        if let Err(error) = download_result {
            let _ = fs::remove_file(&staged);
            return Err(error);
        }

        ensure_safe_target(&root, &file.path, true)?;
        replace_file(&staged, &target)?;
        downloaded = downloaded
            .checked_add(file.size)
            .ok_or_else(|| "Размер обновления переполнен.".to_string())?;
        info!("Updated managed game file {}", file.path);
        emit_progress(
            window,
            progress(
                "installing",
                "Файл установлен. Пользовательские текстуры не трогал.",
                &manifest.version,
                &file.path,
                percent_between(downloaded, total_bytes, 0, 95),
                downloaded,
                total_bytes,
                (index + 1) as u32,
                changed.len() as u32,
            ),
        )?;
    }

    remove_obsolete_managed_files(&root, manifest)?;
    Ok((changed.len() as u32, downloaded))
}

async fn download_managed_file(
    client: &Client,
    managed_file: &ManagedFile,
    destination: &Path,
    window: &Window,
    manifest: &BuildManifest,
    already_downloaded: u64,
    total_bytes: u64,
    completed_files: u32,
    total_files: u32,
) -> std::result::Result<(), String> {
    let url = format!("{FILES_URL}{}", managed_file.path);
    let mut response = get_checked_response(client, &url).await?;
    if response
        .content_length()
        .map_or(false, |length| length != managed_file.size)
    {
        return Err(format!(
            "Размер файла {} не совпадает с манифестом.",
            managed_file.path
        ));
    }

    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(destination)
        .map_err(|error| format!("Не удалось создать временный файл: {error}"))?;
    let mut hasher = Sha256::new();
    let mut downloaded = 0u64;
    let mut last_reported = 0u64;

    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|error| format!("Не удалось загрузить {}: {error}", managed_file.path))?
    {
        let chunk_size = u64::try_from(chunk.len())
            .map_err(|_| "Размер блока файла не помещается в число.".to_string())?;
        downloaded = downloaded
            .checked_add(chunk_size)
            .ok_or_else(|| "Размер файла обновления переполнен.".to_string())?;
        if downloaded > managed_file.size {
            return Err(format!(
                "Сервер передал файл {} больше подписанного размера.",
                managed_file.path
            ));
        }
        file.write_all(&chunk)
            .map_err(|error| format!("Не удалось сохранить {}: {error}", managed_file.path))?;
        hasher.update(&chunk);

        let overall_bytes = already_downloaded
            .checked_add(downloaded)
            .ok_or_else(|| "Размер обновления переполнен.".to_string())?;
        let percent = percent_between(overall_bytes, total_bytes, 0, 95);
        if downloaded.saturating_sub(last_reported) >= DOWNLOAD_CHUNK_BYTES
            || downloaded == managed_file.size
        {
            emit_progress(
                window,
                progress(
                    "downloading",
                    "Загружаю файлы. Чья личная жизнь синхронизировалась — не спрашивайте.",
                    &manifest.version,
                    &managed_file.path,
                    percent,
                    overall_bytes,
                    total_bytes,
                    completed_files,
                    total_files,
                ),
            )?;
            last_reported = downloaded;
        }
    }

    file.sync_all()
        .map_err(|error| format!("Не удалось сохранить {}: {error}", managed_file.path))?;
    if downloaded != managed_file.size {
        return Err(format!(
            "Файл {} загружен не полностью: получено {} из {} байт.",
            managed_file.path, downloaded, managed_file.size
        ));
    }
    let actual_hash = hex_lower(&hasher.finalize());
    if !actual_hash.eq_ignore_ascii_case(&managed_file.sha256) {
        return Err(format!(
            "Файл {} не прошёл проверку SHA-256.",
            managed_file.path
        ));
    }
    Ok(())
}

fn ensure_safe_target(
    root: &Path,
    relative: &str,
    create_parent: bool,
) -> std::result::Result<(), String> {
    validate_managed_path(relative)?;
    let components: Vec<_> = Path::new(relative).components().collect();
    let mut current = root.to_path_buf();
    for component in components.iter().take(components.len().saturating_sub(1)) {
        current.push(component.as_os_str());
        match fs::symlink_metadata(&current) {
            Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_dir() => {
                return Err(format!(
                    "Обновление остановлено: папка {} заменена ссылкой или файлом.",
                    current.display()
                ))
            }
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound && create_parent => {
                fs::create_dir(&current).map_err(|create_error| {
                    format!(
                        "Не удалось создать папку {}: {create_error}",
                        current.display()
                    )
                })?;
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
            Err(error) => {
                return Err(format!(
                    "Не удалось проверить папку {}: {error}",
                    current.display()
                ))
            }
        }
    }

    current.push(
        components
            .last()
            .ok_or_else(|| "У файла обновления отсутствует имя.".to_string())?
            .as_os_str(),
    );
    if let Ok(metadata) = fs::symlink_metadata(&current) {
        if metadata.file_type().is_symlink() || !metadata.is_file() {
            return Err(format!(
                "Обновление остановлено: путь {} занят ссылкой или папкой.",
                current.display()
            ));
        }
    }
    Ok(())
}

fn replace_file(staged: &Path, target: &Path) -> std::result::Result<(), String> {
    if !target.exists() {
        fs::rename(staged, target)
            .map_err(|error| format!("Не удалось установить {}: {error}", target.display()))?;
        return Ok(());
    }

    let backup = unique_sibling_path(target, "kirkawa-backup")?;
    fs::rename(target, &backup)
        .map_err(|error| format!("Не удалось сохранить предыдущий файл: {error}"))?;
    if let Err(error) = fs::rename(staged, target) {
        if let Err(restore_error) = fs::rename(&backup, target) {
            return Err(format!(
                "Не удалось установить файл ({error}) или восстановить его копию ({restore_error})."
            ));
        }
        return Err(format!(
            "Не удалось установить файл {}: {error}",
            target.display()
        ));
    }
    fs::remove_file(backup).map_err(|error| {
        format!("Файл установлен, но не удалось удалить его резервную копию: {error}")
    })?;
    Ok(())
}

fn unique_sibling_path(file: &Path, label: &str) -> std::result::Result<PathBuf, String> {
    let file_name = file
        .file_name()
        .ok_or_else(|| "Не удалось определить имя временного файла.".to_string())?
        .to_string_lossy();
    for attempt in 0..8u8 {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|error| format!("Не удалось получить время системы: {error}"))?
            .as_nanos();
        let candidate = file.with_file_name(format!(
            ".{file_name}.{label}.{}.{nonce}.{attempt}",
            std::process::id()
        ));
        if !candidate.exists() {
            return Ok(candidate);
        }
    }
    Err("Не удалось создать уникальное временное имя.".to_string())
}

fn remove_obsolete_managed_files(
    root: &Path,
    manifest: &BuildManifest,
) -> std::result::Result<(), String> {
    let state_path = manifest_state_path()?;
    if !state_path.exists() {
        return Ok(());
    }

    let state_bytes = fs::read(&state_path)
        .map_err(|error| format!("Не удалось прочитать состояние обновлений: {error}"))?;
    let previous_state: LocalManifestState = match serde_json::from_slice(&state_bytes) {
        Ok(state) => state,
        Err(error) => {
            warn!(
                "Ignoring invalid previous launcher manifest state: {}",
                error
            );
            return Ok(());
        }
    };
    let previous_bytes = previous_state.manifest.as_bytes();
    if let Err(error) = verify_manifest_signature(
        previous_bytes,
        &previous_state.signature,
        &SIGNING_PUBLIC_KEY,
    ) {
        warn!(
            "Keeping old managed files because saved manifest is not trusted: {}",
            error
        );
        return Ok(());
    }
    let previous: BuildManifest = match serde_json::from_slice(previous_bytes) {
        Ok(previous) => previous,
        Err(error) => {
            warn!("Ignoring unparsable previous launcher manifest: {}", error);
            return Ok(());
        }
    };
    validate_manifest(&previous)?;

    let current_paths: HashSet<_> = manifest
        .files
        .iter()
        .map(|file| file.path.to_ascii_lowercase())
        .collect();
    for previous_file in previous.files {
        if current_paths.contains(&previous_file.path.to_ascii_lowercase()) {
            continue;
        }
        ensure_safe_target(root, &previous_file.path, false)?;
        let obsolete = root.join(&previous_file.path);
        match fs::remove_file(&obsolete) {
            Ok(()) => info!("Removed obsolete managed game file {}", previous_file.path),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => {
                return Err(format!(
                    "Не удалось удалить только старый управляемый файл {}: {error}",
                    previous_file.path
                ))
            }
        }
    }

    Ok(())
}

fn save_manifest_state(manifest: &[u8], signature: &str) -> std::result::Result<(), String> {
    let path = manifest_state_path()?;
    let parent = path
        .parent()
        .ok_or_else(|| "Не удалось определить папку состояния обновлений.".to_string())?;
    fs::create_dir_all(parent)
        .map_err(|error| format!("Не удалось создать папку обновлений: {error}"))?;

    let state = LocalManifestState {
        manifest: std::str::from_utf8(manifest)
            .map_err(|_| "Подписанный манифест не является UTF-8.".to_string())?
            .to_string(),
        signature: signature.to_string(),
    };
    let bytes = serde_json::to_vec(&state)
        .map_err(|error| format!("Не удалось сохранить состояние обновлений: {error}"))?;
    let staged = unique_sibling_path(&path, "kirkawa-state")?;
    let write_result = (|| {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&staged)
            .map_err(|error| format!("Не удалось создать состояние обновлений: {error}"))?;
        file.write_all(&bytes)
            .map_err(|error| format!("Не удалось сохранить состояние обновлений: {error}"))?;
        file.sync_all().map_err(|error| {
            format!("Не удалось завершить запись состояния обновлений: {error}")
        })?;
        replace_file(&staged, &path)
    })();
    if write_result.is_err() {
        let _ = fs::remove_file(&staged);
    }
    write_result
}

fn manifest_state_path() -> std::result::Result<PathBuf, String> {
    let data_dir = dirs_next::data_local_dir()
        .ok_or_else(|| "Не удалось определить папку данных пользователя.".to_string())?;
    Ok(data_dir
        .join(crate::constants::DATA_DIR_NAME)
        .join("kirkawa-managed-files.json"))
}

fn hash_file(path: &Path) -> std::result::Result<String, String> {
    let mut file = File::open(path)
        .map_err(|error| format!("Не удалось открыть файл {}: {error}", path.display()))?;
    let mut hasher = Sha256::new();
    let mut buffer = [0u8; 64 * 1024];
    loop {
        let count = file
            .read(&mut buffer)
            .map_err(|error| format!("Не удалось проверить файл {}: {error}", path.display()))?;
        if count == 0 {
            break;
        }
        hasher.update(&buffer[..count]);
    }
    Ok(hex_lower(&hasher.finalize()))
}

fn hex_lower(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        output.push(HEX[(byte >> 4) as usize] as char);
        output.push(HEX[(byte & 0x0f) as usize] as char);
    }
    output
}

fn progress(
    phase: &str,
    message: &str,
    version: &str,
    current_file: &str,
    percent: u8,
    downloaded_bytes: u64,
    total_bytes: u64,
    completed_files: u32,
    total_files: u32,
) -> UpdateProgress {
    UpdateProgress {
        phase: phase.to_string(),
        message: message.to_string(),
        version: version.to_string(),
        current_file: current_file.to_string(),
        percent: percent.min(100),
        downloaded_bytes,
        total_bytes,
        completed_files,
        total_files,
    }
}

fn percent_between(current: u64, total: u64, start: u8, end: u8) -> u8 {
    if total == 0 {
        return end;
    }
    let range = f64::from(end.saturating_sub(start));
    let fraction = (current.min(total) as f64) / (total as f64);
    start + (fraction.mul_add(range, 0.0).floor() as u8)
}

fn emit_progress(window: &Window, update: UpdateProgress) -> std::result::Result<(), String> {
    window
        .emit("kirkawa-update-progress", update)
        .map_err(|error| format!("Не удалось показать ход обновления: {error}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use ring::signature::{Ed25519KeyPair, KeyPair};

    #[test]
    fn accepts_only_scoped_managed_paths() {
        assert!(validate_managed_path("SAMP/custom.img").is_ok());
        assert!(validate_managed_path("SAMP/maps/objects.ide").is_ok());
        assert!(validate_managed_path("../models/gta3.img").is_err());
        assert!(validate_managed_path("SAMP/../../gta_sa.exe").is_err());
        assert!(validate_managed_path("C:/Windows/win.ini").is_err());
        assert!(validate_managed_path("SAMP\\custom.img").is_err());
        assert!(validate_managed_path("SAMP/NUL.ide").is_err());
    }

    #[test]
    fn rejects_archive_paths_that_can_escape_the_install_folder() {
        assert!(validate_archive_path("models/gta3.img").is_ok());
        assert!(validate_archive_path("../Windows/System32/file").is_err());
        assert!(validate_archive_path("C:/Windows/file").is_err());
        assert!(validate_archive_path("\\\\server\\share\\file").is_err());
        assert!(validate_archive_path("models/COM1.img").is_err());
    }

    #[test]
    fn verifies_the_exact_signed_manifest_bytes() {
        let signing_key = Ed25519KeyPair::from_seed_unchecked(&[7; 32]).unwrap();
        let signature = STANDARD.encode(signing_key.sign(b"{\"version\":\"1\"}").as_ref());
        let public_key: [u8; 32] = signing_key.public_key().as_ref().try_into().unwrap();

        assert!(verify_manifest_signature(b"{\"version\":\"1\"}", &signature, &public_key).is_ok());
        assert!(
            verify_manifest_signature(b"{\"version\":\"2\"}", &signature, &public_key).is_err()
        );
        assert!(
            verify_manifest_signature(b"{\"version\":\"1\"}", &signature, &SIGNING_PUBLIC_KEY)
                .is_err()
        );
    }

    #[test]
    fn rejects_release_notes_that_are_empty_or_contain_controls() {
        assert!(validate_release_notes("Исправлены ошибки.\nДобавлена карта.").is_ok());
        assert!(validate_release_notes(" \n\t").is_err());
        assert!(validate_release_notes("недопустимый\u{0001}символ").is_err());
    }

    #[test]
    fn rejects_malformed_package_metadata_and_duplicate_managed_paths() {
        let manifest: BuildManifest = serde_json::from_str(
            r#"{"version":"1.0.0","gamePackage":{"size":1,"unpackedSize":1,"sha256":"0000000000000000000000000000000000000000000000000000000000000000"},"files":[{"path":"SAMP/file","size":0,"sha256":"0000000000000000000000000000000000000000000000000000000000000000"},{"path":"samp/File","size":0,"sha256":"0000000000000000000000000000000000000000000000000000000000000000"}]}"#,
        )
        .unwrap();

        assert!(validate_manifest(&manifest).is_err());
    }

    #[test]
    fn progress_percentage_is_clamped_to_the_requested_range() {
        assert_eq!(percent_between(1, 2, 20, 80), 50);
        assert_eq!(percent_between(3, 2, 20, 80), 80);
        assert_eq!(percent_between(1, 0, 20, 80), 80);
    }
}
