use base64::Engine;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use tauri::State;

mod error;
pub use error::{AppError, ErrorResponse};

mod decrypt;
use decrypt::DatDecryptor;

#[cfg(windows)]
pub mod dll;

// 配置文件路径
const CONFIG_FILE: &str = "config.json";

// 全局状态
#[derive(Default)]
pub struct AppState {
    root_dir: Mutex<Option<PathBuf>>,
    xor_key: Mutex<u8>,
    aes_key: Mutex<Vec<u8>>,
}

// 配置结构
#[derive(Serialize, Deserialize)]
struct Config {
    xor: u8,
    aes: String,
}

// 目录树节点
#[derive(Serialize)]
struct TreeNode {
    name: String,
    path: String,
    children: Vec<TreeNode>,
}

// 读取配置文件
fn read_key_from_config() -> (u8, Vec<u8>) {
    if let Ok(content) = fs::read_to_string(CONFIG_FILE) {
        if let Ok(config) = serde_json::from_str::<Config>(&content) {
            let aes_bytes = config.aes.as_bytes().to_vec();
            let aes_key = if aes_bytes.len() >= 16 {
                aes_bytes[..16].to_vec()
            } else {
                aes_bytes
            };
            return (config.xor, aes_key);
        }
    }
    (0, vec![])
}

// 保存配置文件
fn save_key_to_config(xor: u8, aes: &str) -> Result<(), AppError> {
    let config = Config {
        xor,
        aes: aes.to_string(),
    };
    let json = serde_json::to_string_pretty(&config)
        .map_err(|e| AppError::ConfigSerializeError(e.to_string()))?;
    fs::write(CONFIG_FILE, json).map_err(|e| AppError::FileWriteError(e.to_string()))?;
    Ok(())
}

// 打开文件夹对话框
#[tauri::command]
async fn open_folder_dialog(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
) -> Result<String, String> {
    use tauri_plugin_dialog::DialogExt;

    let folder = app.dialog().file().blocking_pick_folder();

    if let Some(path) = folder {
        let path_buf = path
            .as_path()
            .ok_or(AppError::InvalidPath("对话框返回无效路径".to_string()))?;
        let path_str = path_buf.to_string_lossy().to_string();

        // 更新状态
        *state.root_dir.lock().unwrap() = Some(path_buf.to_path_buf());

        // 读取配置文件中的密钥
        let (xor, aes) = read_key_from_config();
        *state.xor_key.lock().unwrap() = xor;
        *state.aes_key.lock().unwrap() = aes;

        Ok(path_str)
    } else {
        Err(String::from(AppError::NoFolderSelected))
    }
}

// 获取文件夹树
#[tauri::command]
fn get_folder_tree(state: State<AppState>) -> Result<TreeNode, String> {
    let root_dir = state.root_dir.lock().unwrap();
    let root_path = root_dir
        .as_ref()
        .ok_or(AppError::RootDirNotSet)
        .map_err(|e| String::from(e))?;

    fn build_tree(dir_path: &Path) -> Result<TreeNode, AppError> {
        let name = dir_path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("")
            .to_string();
        let path = dir_path.to_string_lossy().to_string();
        let mut children = Vec::new();

        let entries = match fs::read_dir(dir_path) {
            Ok(entries) => entries,
            Err(e) => {
                log::warn!("无法读取目录 {}: {}", path, e);
                return Ok(TreeNode {
                    name,
                    path,
                    children,
                });
            }
        };

        for entry in entries.flatten() {
            let file_type = match entry.file_type() {
                Ok(ft) => ft,
                Err(_) => continue,
            };

            if !file_type.is_dir() {
                continue;
            }

            let child = match build_tree(&entry.path()) {
                Ok(child) => child,
                Err(e) => {
                    log::warn!("递归构建树失败: {}", e);
                    continue;
                }
            };

            children.push(child);
        }

        Ok(TreeNode {
            name,
            path,
            children,
        })
    }

    build_tree(root_path).map_err(|e| String::from(e))
}

// 获取文件夹中的图片
#[tauri::command]
fn get_images_in_folder(
    folder_path: String,
    state: State<AppState>,
) -> Result<Vec<String>, String> {
    let root_dir = state.root_dir.lock().unwrap();
    let root_path = root_dir
        .as_ref()
        .ok_or(AppError::RootDirNotSet)
        .map_err(|e| String::from(e))?;

    let folder = Path::new(&folder_path);
    if !folder.starts_with(root_path) {
        return Err(String::from(AppError::InvalidPath(folder_path)));
    }

    let mut relative_paths = Vec::new();

    let entries = match fs::read_dir(folder) {
        Ok(entries) => entries,
        Err(e) => {
            log::warn!("无法读取文件夹 {}: {}", folder_path, e);
            return Ok(relative_paths);
        }
    };

    for entry in entries.flatten() {
        let file_type = match entry.file_type() {
            Ok(ft) => ft,
            Err(_) => continue,
        };

        if !file_type.is_file() {
            continue;
        }

        let path = entry.path();
        let filename = match path.file_name().and_then(|n| n.to_str()) {
            Some(name) => name,
            None => continue,
        };

        // 检查是否是 .dat 文件或 Sns 缓存文件
        let is_dat = filename.to_lowercase().ends_with(".dat");
        let is_sns = is_valid_sns_filename(filename);

        if !is_dat && !is_sns {
            continue;
        }

        let rel_path = match path.strip_prefix(root_path) {
            Ok(p) => p,
            Err(_) => continue,
        };

        relative_paths.push(rel_path.to_string_lossy().to_string());
    }

    Ok(relative_paths)
}

// 检查是否是有效的 Sns 文件名
fn is_valid_sns_filename(filename: &str) -> bool {
    let name = filename.trim_end_matches("_t");
    let len = name.len();
    (len == 30 || len == 32) && name.chars().all(|c| c.is_alphanumeric())
}

// 解密 DAT 文件
#[tauri::command]
fn decrypt_dat_file(file_path: String, state: State<AppState>) -> Result<String, String> {
    let root_dir = state.root_dir.lock().unwrap();
    let root_path = root_dir
        .as_ref()
        .ok_or(AppError::RootDirNotSet)
        .map_err(|e| String::from(e))?;

    let full_path = root_path.join(&file_path);

    if !full_path.exists() {
        return Err(String::from(AppError::FileNotFound(file_path)));
    }

    let xor_key = *state.xor_key.lock().unwrap();
    let aes_key = state.aes_key.lock().unwrap();

    // 只有当 AES 密钥长度为 16 字节时才使用它
    let aes_key_option = if aes_key.len() == 16 {
        Some(aes_key.as_slice())
    } else {
        None
    };

    // 解密文件
    let decrypted_data = DatDecryptor::decrypt(&full_path, xor_key, aes_key_option)
        .map_err(|e| String::from(AppError::DecryptFailed(format!("{:?}", e))))?;

    // 转换为 base64
    let base64_data = base64::engine::general_purpose::STANDARD.encode(&decrypted_data);

    Ok(base64_data)
}

// 更新密钥
#[tauri::command]
fn update_keys(xor: u8, aes: String, state: State<AppState>) -> Result<(), String> {
    *state.xor_key.lock().unwrap() = xor;

    let aes_bytes = aes.as_bytes().to_vec();
    let aes_key = if aes_bytes.len() >= 16 {
        aes_bytes[..16].to_vec()
    } else {
        aes_bytes
    };
    *state.aes_key.lock().unwrap() = aes_key;

    // 保存到配置文件
    save_key_to_config(xor, &aes).map_err(|e| String::from(e))?;

    Ok(())
}

// 获取当前密钥
#[tauri::command]
fn get_keys(state: State<AppState>) -> Result<(u8, String), String> {
    let xor = *state.xor_key.lock().unwrap();
    let aes = state.aes_key.lock().unwrap();
    let aes_str = String::from_utf8_lossy(&aes).to_string();
    Ok((xor, aes_str))
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .manage(AppState::default())
        .invoke_handler(tauri::generate_handler![
            open_folder_dialog,
            get_folder_tree,
            get_images_in_folder,
            decrypt_dat_file,
            update_keys,
            get_keys
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
