use base64::Engine;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
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
    // 图片缓存：存储解密后的图片数据
    image_cache: Arc<Mutex<HashMap<String, Vec<u8>>>>,
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

// 图片文件信息
#[derive(Serialize, Clone)]
struct ImageInfo {
    path: String,
    name: String,
    size: u64,
    modified: u64,
    is_thumbnail: bool,
}

// 批量图片响应（带解密数据）
#[derive(Serialize)]
struct ImageBatch {
    images: Vec<ImageWithData>,
    total: usize,
    page: usize,
    page_size: usize,
    has_more: bool,
}

// 带解密数据的图片信息（现在只包含元数据，不包含图片数据）
#[derive(Serialize)]
struct ImageWithData {
    path: String,
    name: String,
    size: u64,
    modified: u64,
    is_thumbnail: bool,
    mime_type: String,
    // 用于前端获取图片的唯一标识符
    image_id: String,
}

// 读取配置文件
fn read_key_from_config() -> (u8, Vec<u8>) {
    let content = match fs::read_to_string(CONFIG_FILE) {
        Ok(c) => c,
        Err(_) => return (0, vec![]),
    };

    let config = match serde_json::from_str::<Config>(&content) {
        Ok(c) => c,
        Err(_) => return (0, vec![]),
    };

    let aes_bytes = config.aes.as_bytes().to_vec();
    let aes_key = if aes_bytes.len() >= 16 {
        aes_bytes[..16].to_vec()
    } else {
        aes_bytes
    };

    (config.xor, aes_key)
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
) -> Result<Vec<ImageInfo>, String> {
    let root_dir = state.root_dir.lock().unwrap();
    let root_path = root_dir
        .as_ref()
        .ok_or(AppError::RootDirNotSet)
        .map_err(|e| String::from(e))?;

    let folder = Path::new(&folder_path);
    if !folder.starts_with(root_path) {
        return Err(String::from(AppError::InvalidPath(folder_path)));
    }

    let mut images = Vec::new();

    let entries = match fs::read_dir(folder) {
        Ok(entries) => entries,
        Err(e) => {
            log::warn!("无法读取文件夹 {}: {}", folder_path, e);
            return Ok(images);
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

        // 检查是否是缩略图
        let is_thumbnail = filename.to_lowercase().ends_with("_t.dat");

        let rel_path = match path.strip_prefix(root_path) {
            Ok(p) => p,
            Err(_) => continue,
        };

        // 获取文件元数据
        let metadata = match fs::metadata(&path) {
            Ok(m) => m,
            Err(_) => continue,
        };

        let size = metadata.len();
        let modified = metadata
            .modified()
            .ok()
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| d.as_secs())
            .unwrap_or(0);

        images.push(ImageInfo {
            path: rel_path.to_string_lossy().to_string(),
            name: filename.to_string(),
            size,
            modified,
            is_thumbnail,
        });
    }

    Ok(images)
}

// 批量获取图片（带排序、筛选和分页）
#[tauri::command]
async fn get_images_batch(
    folder_path: String,
    page: usize,
    page_size: usize,
    sort_by: String,
    sort_order: String,
    hide_thumbnails: bool,
    state: State<'_, AppState>,
) -> Result<ImageBatch, String> {
    let root_dir = state.root_dir.lock().unwrap().clone();
    let root_path = root_dir
        .as_ref()
        .ok_or(AppError::RootDirNotSet)
        .map_err(|e| String::from(e))?
        .clone();

    let folder = Path::new(&folder_path);
    if !folder.starts_with(&root_path) {
        return Err(String::from(AppError::InvalidPath(folder_path)));
    }

    // 获取所有图片信息
    let mut images = Vec::new();
    let entries = match fs::read_dir(folder) {
        Ok(entries) => entries,
        Err(e) => {
            log::warn!("无法读取文件夹 {}: {}", folder_path, e);
            return Ok(ImageBatch {
                images: vec![],
                total: 0,
                page,
                page_size,
                has_more: false,
            });
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

        let is_dat = filename.to_lowercase().ends_with(".dat");
        let is_sns = is_valid_sns_filename(filename);

        if !is_dat && !is_sns {
            continue;
        }

        let is_thumbnail = filename.to_lowercase().ends_with("_t.dat") || filename.ends_with("_t");

        // 筛选缩略图
        if hide_thumbnails && is_thumbnail {
            continue;
        }

        let rel_path = match path.strip_prefix(&root_path) {
            Ok(p) => p,
            Err(_) => continue,
        };

        let metadata = match fs::metadata(&path) {
            Ok(m) => m,
            Err(_) => continue,
        };

        let size = metadata.len();
        let modified = metadata
            .modified()
            .ok()
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| d.as_secs())
            .unwrap_or(0);

        images.push(ImageInfo {
            path: rel_path.to_string_lossy().to_string(),
            name: filename.to_string(),
            size,
            modified,
            is_thumbnail,
        });
    }

    // 去重：为同一hash的图片组选择最佳版本（优先级：_t > 无后缀 > _h）
    images = deduplicate_images_by_hash(images);

    // 排序
    match (sort_by.as_str(), sort_order.as_str()) {
        ("name", "asc") => images.sort_by(|a, b| a.name.cmp(&b.name)),
        ("name", "desc") => images.sort_by(|a, b| b.name.cmp(&a.name)),
        ("time", "asc") => images.sort_by(|a, b| a.modified.cmp(&b.modified)),
        ("time", "desc") => images.sort_by(|a, b| b.modified.cmp(&a.modified)),
        ("size", "asc") => images.sort_by(|a, b| a.size.cmp(&b.size)),
        ("size", "desc") => images.sort_by(|a, b| b.size.cmp(&a.size)),
        _ => {}
    }

    let total = images.len();
    let start = page * page_size;
    let end = (start + page_size).min(total);
    let has_more = end < total;

    // 分页
    let page_images: Vec<ImageInfo> = images.into_iter().skip(start).take(page_size).collect();

    // 批量解密
    let xor_key = *state.xor_key.lock().unwrap();
    let aes_key = state.aes_key.lock().unwrap().clone();
    let aes_key_option = if aes_key.len() == 16 {
        Some(aes_key)
    } else {
        None
    };

    // 使用 tokio 并发解密并缓存
    let mut tasks = Vec::new();
    let cache = state.image_cache.clone();

    for img_info in page_images {
        let root_path_clone = root_path.clone();
        let xor_key_clone = xor_key;
        let aes_key_clone = aes_key_option.clone();
        let cache_clone = cache.clone();

        let task = tokio::task::spawn_blocking(move || {
            let full_path = root_path_clone.join(&img_info.path);

            let decrypted_data =
                match DatDecryptor::decrypt(&full_path, xor_key_clone, aes_key_clone.as_deref()) {
                    Ok(data) => data,
                    Err(e) => {
                        log::warn!("解密失败 {}: {:?}", img_info.path, e);
                        return Err(format!("解密失败: {:?}", e));
                    }
                };

            let (normalized_data, mime_type) = normalize_decrypted_image(decrypted_data);
            let image_id = img_info.path.clone();

            let mut cache_map = cache_clone.lock().unwrap();
            cache_map.insert(image_id.clone(), normalized_data);

            Ok(ImageWithData {
                path: img_info.path,
                name: img_info.name,
                size: img_info.size,
                modified: img_info.modified,
                is_thumbnail: img_info.is_thumbnail,
                image_id,
                mime_type,
            })
        });

        tasks.push(task);
    }

    // 等待所有任务完成
    let mut images_with_data = Vec::new();
    for task in tasks {
        if let Ok(Ok(img)) = task.await {
            images_with_data.push(img);
        }
    }

    Ok(ImageBatch {
        images: images_with_data,
        total,
        page,
        page_size,
        has_more,
    })
}

// 检测图片 MIME 类型
fn detect_mime_type(data: &[u8]) -> &'static str {
    if data.len() < 4 {
        return "application/octet-stream";
    }

    // JPEG: FF D8 FF
    if data.len() >= 3 && data[0] == 0xFF && data[1] == 0xD8 && data[2] == 0xFF {
        return "image/jpeg";
    }

    // PNG: 89 50 4E 47
    if data.len() >= 4 && data[0] == 0x89 && data[1] == 0x50 && data[2] == 0x4E && data[3] == 0x47 {
        return "image/png";
    }

    // GIF: 47 49 46
    if data.len() >= 3 && data[0] == 0x47 && data[1] == 0x49 && data[2] == 0x46 {
        return "image/gif";
    }

    // WebP: 52 49 46 46 ... 57 45 42 50
    if data.len() >= 12
        && data[0] == 0x52
        && data[1] == 0x49
        && data[2] == 0x46
        && data[3] == 0x46
        && data[8] == 0x57
        && data[9] == 0x45
        && data[10] == 0x42
        && data[11] == 0x50
    {
        return "image/webp";
    }

    "image/jpeg" // 默认为 JPEG
}

/// 对解密后的图片数据进行规范化处理
///
/// - 检测带有 WXGF 头的数据并尝试通过 DLL 转换成标准图片
/// - 返回转换后的数据及其 MIME 类型
fn normalize_decrypted_image(data: Vec<u8>) -> (Vec<u8>, String) {
    #[cfg(windows)]
    if data.len() < 4 {
        let mime = detect_mime_type(&data).to_string();
        return (data, mime);
    }

    let header = &data[..4];
    if header != b"wxgf" && header != b"WXGF" {
        let mime = detect_mime_type(&data).to_string();
        return (data, mime);
    }

    match crate::dll::wxam_to_image(&data, "jpeg") {
        Ok(converted) => {
            log::debug!(
                "检测到 WXGF 图片,已通过 DLL 转换,输出大小: {} 字节",
                converted.len()
            );
            let mime = detect_mime_type(&converted).to_string();
            return (converted, mime);
        }
        Err(err) => {
            log::warn!("WXGF 图片转换失败: {}", err);
        }
    }

    let mime = detect_mime_type(&data).to_string();
    (data, mime)
}

// 提取文件名的hash部分（不包含后缀）
fn extract_hash_from_filename(filename: &str) -> String {
    let name_without_ext = filename
        .trim_end_matches(".dat")
        .trim_end_matches("_t")
        .trim_end_matches("_h");
    name_without_ext.to_string()
}

// 获取图片版本优先级（数字越小优先级越高）
fn get_image_priority(filename: &str) -> u8 {
    let lower = filename.to_lowercase();
    if lower.ends_with("_t.dat") || lower.ends_with("_t") {
        0 // 缩略图优先级最高
    } else if lower.ends_with("_h.dat") || lower.ends_with("_h") {
        2 // 原图优先级最低
    } else {
        1 // 中等规格优先级居中
    }
}

// 去重：为同一hash的图片组选择最佳版本
fn deduplicate_images_by_hash(images: Vec<ImageInfo>) -> Vec<ImageInfo> {
    use std::collections::HashMap;

    let mut hash_map: HashMap<String, ImageInfo> = HashMap::new();

    for img in images {
        let hash = extract_hash_from_filename(&img.name);

        // 如果这个hash还没有记录，直接插入
        let Some(existing) = hash_map.get(&hash) else {
            hash_map.insert(hash, img);
            continue;
        };

        // 如果当前图片优先级更高，就更新
        let current_priority = get_image_priority(&img.name);
        let existing_priority = get_image_priority(&existing.name);

        if current_priority < existing_priority {
            hash_map.insert(hash, img);
        }
    }

    // 收集所有选中的图片
    hash_map.into_values().collect()
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

// 获取缓存中的图片数据
#[tauri::command]
fn get_image_data(image_id: String, state: State<AppState>) -> Result<Vec<u8>, String> {
    let cache = state.image_cache.lock().unwrap();

    cache
        .get(&image_id)
        .cloned()
        .ok_or_else(|| format!("图片不在缓存中: {}", image_id))
}

// 清除图片缓存（可选，用于释放内存）
#[tauri::command]
fn clear_image_cache(state: State<AppState>) -> Result<(), String> {
    let mut cache = state.image_cache.lock().unwrap();
    cache.clear();
    Ok(())
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
            get_images_batch,
            decrypt_dat_file,
            update_keys,
            get_keys,
            get_image_data,
            clear_image_cache
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
