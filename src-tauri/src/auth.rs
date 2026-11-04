use reqwest::{redirect::Policy, Client, StatusCode};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    fs,
    path::PathBuf,
    time::Duration,
};

const AUTH_API_URL: &str = "https://auth.kirkawa-games.online/api.php";
const SESSION_FILE: &str = "kirkawa-account-session.bin";

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AccountInfo {
    id: u64,
    login: String,
}

#[derive(Deserialize)]
struct AuthResponse {
    account: AccountInfo,
    token: Option<String>,
}

fn build_client() -> std::result::Result<Client, String> {
    Client::builder()
        .connect_timeout(Duration::from_secs(15))
        .timeout(Duration::from_secs(30))
        .redirect(Policy::none())
        .user_agent("KirkawaLauncher/1.7")
        .build()
        .map_err(|error| format!("Не удалось настроить защищённое соединение с аккаунтами: {error}"))
}

async fn post(
    action: &str,
    fields: Value,
) -> std::result::Result<(StatusCode, Value), String> {
    let mut body = fields;
    let object = body
        .as_object_mut()
        .ok_or_else(|| "Некорректный запрос аккаунта.".to_string())?;
    object.insert("action".to_string(), Value::String(action.to_string()));
    let bytes = serde_json::to_vec(&body)
        .map_err(|error| format!("Не удалось подготовить запрос аккаунта: {error}"))?;
    let response = build_client()?
        .post(AUTH_API_URL)
        .header(reqwest::header::CONTENT_TYPE, "application/json")
        .body(bytes)
        .send()
        .await
        .map_err(|error| format!("Не удалось связаться с сервером аккаунтов: {error}"))?;
    let status = response.status();
    let response_bytes = response
        .bytes()
        .await
        .map_err(|error| format!("Не удалось получить ответ сервера аккаунтов: {error}"))?;
    let body = serde_json::from_slice(&response_bytes)
        .map_err(|error| format!("Сервер аккаунтов вернул некорректный ответ: {error}"))?;
    Ok((status, body))
}

fn api_error(status: StatusCode, body: &Value) -> String {
    body.get("error")
        .and_then(Value::as_str)
        .map(str::to_string)
        .unwrap_or_else(|| format!("Сервис аккаунтов вернул HTTP {status}."))
}

fn parse_account_response(
    status: StatusCode,
    body: Value,
) -> std::result::Result<AuthResponse, String> {
    if !status.is_success() {
        return Err(api_error(status, &body));
    }
    serde_json::from_value(body)
        .map_err(|error| format!("Сервер аккаунтов вернул неполные данные: {error}"))
}

#[tauri::command]
pub async fn register_account(
    login: String,
    password: String,
) -> std::result::Result<AccountInfo, String> {
    let (status, body) = post(
        "register",
        json!({ "login": login, "password": password }),
    )
    .await?;
    let response = parse_account_response(status, body)?;
    let token = response
        .token
        .as_deref()
        .ok_or_else(|| "Сервер не выдал сессию после регистрации.".to_string())?;
    store_session_token(token)?;
    Ok(response.account)
}

#[tauri::command]
pub async fn login_account(
    login: String,
    password: String,
) -> std::result::Result<AccountInfo, String> {
    let (status, body) =
        post("login", json!({ "login": login, "password": password })).await?;
    let response = parse_account_response(status, body)?;
    let token = response
        .token
        .as_deref()
        .ok_or_else(|| "Сервер не выдал сессию после входа.".to_string())?;
    store_session_token(token)?;
    Ok(response.account)
}

#[tauri::command]
pub async fn restore_account() -> std::result::Result<Option<AccountInfo>, String> {
    let token = match load_session_token()? {
        Some(token) => token,
        None => return Ok(None),
    };
    let (status, body) = post("restore", json!({ "token": token })).await?;
    if status == StatusCode::UNAUTHORIZED {
        remove_session_token()?;
        return Ok(None);
    }
    let response = parse_account_response(status, body)?;
    Ok(Some(response.account))
}

#[tauri::command]
pub async fn logout_account() -> std::result::Result<(), String> {
    let token = load_session_token()?;
    remove_session_token()?;
    let Some(token) = token else {
        return Ok(());
    };
    let (status, body) = post("logout", json!({ "token": token })).await?;
    if !status.is_success() {
        return Err(api_error(status, &body));
    }
    Ok(())
}

fn session_file_path() -> std::result::Result<PathBuf, String> {
    let data_dir = dirs_next::data_local_dir()
        .ok_or_else(|| "Не удалось определить папку данных пользователя.".to_string())?;
    Ok(data_dir
        .join(crate::constants::DATA_DIR_NAME)
        .join(SESSION_FILE))
}

fn store_session_token(token: &str) -> std::result::Result<(), String> {
    if token.len() != 64 || !token.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err("Сервер вернул недействительную сессию.".to_string());
    }
    let encrypted = protect_for_current_user(token.as_bytes())?;
    let path = session_file_path()?;
    let parent = path
        .parent()
        .ok_or_else(|| "Не удалось определить папку сессии аккаунта.".to_string())?;
    fs::create_dir_all(parent)
        .map_err(|error| format!("Не удалось создать папку сессии аккаунта: {error}"))?;
    fs::write(path, encrypted)
        .map_err(|error| format!("Не удалось сохранить сессию аккаунта: {error}"))
}

fn load_session_token() -> std::result::Result<Option<String>, String> {
    let path = session_file_path()?;
    let encrypted = match fs::read(path) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => {
            return Err(format!("Не удалось прочитать сохранённую сессию аккаунта: {error}"))
        }
    };
    let decrypted = unprotect_for_current_user(&encrypted)?;
    let token = String::from_utf8(decrypted)
        .map_err(|_| "Сохранённая сессия аккаунта повреждена.".to_string())?;
    if token.len() != 64 || !token.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err("Сохранённая сессия аккаунта повреждена.".to_string());
    }
    Ok(Some(token))
}

fn remove_session_token() -> std::result::Result<(), String> {
    let path = session_file_path()?;
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(format!("Не удалось удалить локальную сессию аккаунта: {error}")),
    }
}

#[cfg(windows)]
fn protect_for_current_user(bytes: &[u8]) -> std::result::Result<Vec<u8>, String> {
    use windows_sys::Win32::{
        Foundation::LocalFree,
        Security::Cryptography::{
            CryptProtectData, CRYPT_INTEGER_BLOB, CRYPTPROTECT_UI_FORBIDDEN,
        },
    };

    let input = CRYPT_INTEGER_BLOB {
        cbData: bytes.len() as u32,
        pbData: bytes.as_ptr() as *mut u8,
    };
    let mut output = CRYPT_INTEGER_BLOB {
        cbData: 0,
        pbData: std::ptr::null_mut(),
    };
    let success = unsafe {
        CryptProtectData(
            &input,
            std::ptr::null(),
            std::ptr::null(),
            std::ptr::null(),
            std::ptr::null(),
            CRYPTPROTECT_UI_FORBIDDEN,
            &mut output,
        )
    };
    if success == 0 {
        return Err(format!(
            "Не удалось защитить сессию аккаунта средствами Windows: {}",
            std::io::Error::last_os_error()
        ));
    }
    let protected = unsafe {
        std::slice::from_raw_parts(output.pbData, output.cbData as usize).to_vec()
    };
    unsafe {
        LocalFree(output.pbData.cast());
    }
    Ok(protected)
}

#[cfg(windows)]
fn unprotect_for_current_user(bytes: &[u8]) -> std::result::Result<Vec<u8>, String> {
    use windows_sys::Win32::{
        Foundation::LocalFree,
        Security::Cryptography::{
            CryptUnprotectData, CRYPT_INTEGER_BLOB, CRYPTPROTECT_UI_FORBIDDEN,
        },
    };

    let input = CRYPT_INTEGER_BLOB {
        cbData: bytes.len() as u32,
        pbData: bytes.as_ptr() as *mut u8,
    };
    let mut output = CRYPT_INTEGER_BLOB {
        cbData: 0,
        pbData: std::ptr::null_mut(),
    };
    let success = unsafe {
        CryptUnprotectData(
            &input,
            std::ptr::null_mut(),
            std::ptr::null(),
            std::ptr::null(),
            std::ptr::null(),
            CRYPTPROTECT_UI_FORBIDDEN,
            &mut output,
        )
    };
    if success == 0 {
        return Err(format!(
            "Не удалось открыть сохранённую сессию Windows: {}",
            std::io::Error::last_os_error()
        ));
    }
    let unprotected = unsafe {
        std::slice::from_raw_parts(output.pbData, output.cbData as usize).to_vec()
    };
    unsafe {
        LocalFree(output.pbData.cast());
    }
    Ok(unprotected)
}

#[cfg(not(windows))]
fn protect_for_current_user(_: &[u8]) -> std::result::Result<Vec<u8>, String> {
    Err("Сохранение аккаунта поддерживается только в Windows.".to_string())
}

#[cfg(not(windows))]
fn unprotect_for_current_user(_: &[u8]) -> std::result::Result<Vec<u8>, String> {
    Err("Сохранение аккаунта поддерживается только в Windows.".to_string())
}
