//! 统一的应用程序错误处理模块
//!
//! 该模块定义了应用程序中所有可能的错误类型,为整个应用提供一致的错误处理机制。

use serde::Serialize;
use std::fmt;
use thiserror::Error;

/// 应用程序错误类型
///
/// 统一管理来自所有子模块的错误,包括:
/// - 文件系统操作错误
/// - 解密相关错误
/// - DLL 加载和调用错误
/// - 配置处理错误
#[derive(Error, Debug, Clone)]
pub enum AppError {
    // ===== 文件系统错误 =====
    #[error("文件不存在: {0}")]
    FileNotFound(String),

    #[error("文件读取失败: {0}")]
    FileReadError(String),

    #[error("文件写入失败: {0}")]
    FileWriteError(String),

    #[error("无效的文件夹路径: {0}")]
    InvalidPath(String),

    // ===== 配置错误 =====
    #[error("配置文件格式错误: {0}")]
    ConfigParseError(String),

    #[error("配置序列化失败: {0}")]
    ConfigSerializeError(String),

    #[error("未设置根目录")]
    RootDirNotSet,

    #[error("未选择文件夹")]
    NoFolderSelected,

    // ===== 解密错误 =====
    #[error("不支持的 DAT 版本")]
    UnsupportedDatVersion,

    #[error("DAT 文件格式无效")]
    InvalidDatFormat,

    #[error("DAT 文件头解析失败")]
    DatHeaderParseError,

    #[error("AES 解密失败: {0}")]
    AesDecryptError(String),

    #[error("解密失败: {0}")]
    DecryptFailed(String),

    // ===== DLL 错误 =====
    #[error("DLL 文件不存在: {0}")]
    DllNotFound(String),

    #[error("DLL 加载失败: {0}")]
    DllLoadFailed(String),

    #[error("DLL 函数未正确初始化")]
    DllFunctionNotInitialized,

    #[error("DLL 解码失败,错误代码: {0}")]
    DllDecodeFailed(i64),

    #[error("WXAM 解码失败: {0}")]
    WxamDecodeFailed(String),

    #[error("不支持的图片格式: {0}")]
    UnsupportedImageFormat(String),

    #[error("输入数据不能为空")]
    EmptyInput,

    #[error("输出数据大小无效")]
    InvalidOutputSize,

    // ===== 通用错误 =====
    #[error("内部错误: {0}")]
    Internal(String),
}

/// 用于 Tauri 返回的错误响应
#[derive(Debug, Serialize)]
pub struct ErrorResponse {
    /// 错误代码
    code: String,
    /// 错误消息
    message: String,
}

impl ErrorResponse {
    /// 创建新的错误响应
    pub fn new(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
        }
    }
}

impl fmt::Display for ErrorResponse {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "[{}] {}", self.code, self.message)
    }
}

impl From<AppError> for ErrorResponse {
    fn from(err: AppError) -> Self {
        let (code, message) = err.to_code_and_message();
        ErrorResponse::new(code, message)
    }
}

impl From<std::io::Error> for AppError {
    fn from(err: std::io::Error) -> Self {
        AppError::Internal(err.to_string())
    }
}

impl From<serde_json::Error> for AppError {
    fn from(err: serde_json::Error) -> Self {
        AppError::ConfigParseError(err.to_string())
    }
}

impl AppError {
    /// 将错误转换为错误代码和消息
    ///
    /// 用于向前端返回结构化的错误信息
    pub fn to_code_and_message(&self) -> (String, String) {
        match self {
            // 文件系统错误
            AppError::FileNotFound(path) => (
                "FILE_NOT_FOUND".to_string(),
                format!("文件不存在: {}", path),
            ),
            AppError::FileReadError(msg) => (
                "FILE_READ_ERROR".to_string(),
                format!("文件读取失败: {}", msg),
            ),
            AppError::FileWriteError(msg) => (
                "FILE_WRITE_ERROR".to_string(),
                format!("文件写入失败: {}", msg),
            ),
            AppError::InvalidPath(path) => (
                "INVALID_PATH".to_string(),
                format!("无效的文件夹路径: {}", path),
            ),

            // 配置错误
            AppError::ConfigParseError(msg) => (
                "CONFIG_PARSE_ERROR".to_string(),
                format!("配置文件格式错误: {}", msg),
            ),
            AppError::ConfigSerializeError(msg) => (
                "CONFIG_SERIALIZE_ERROR".to_string(),
                format!("配置序列化失败: {}", msg),
            ),
            AppError::RootDirNotSet => ("ROOT_DIR_NOT_SET".to_string(), "未设置根目录".to_string()),
            AppError::NoFolderSelected => {
                ("NO_FOLDER_SELECTED".to_string(), "未选择文件夹".to_string())
            }

            // 解密错误
            AppError::UnsupportedDatVersion => (
                "UNSUPPORTED_DAT_VERSION".to_string(),
                "不支持的 DAT 版本".to_string(),
            ),
            AppError::InvalidDatFormat => (
                "INVALID_DAT_FORMAT".to_string(),
                "DAT 文件格式无效".to_string(),
            ),
            AppError::DatHeaderParseError => (
                "DAT_HEADER_PARSE_ERROR".to_string(),
                "DAT 文件头解析失败".to_string(),
            ),
            AppError::AesDecryptError(msg) => (
                "AES_DECRYPT_ERROR".to_string(),
                format!("AES 解密失败: {}", msg),
            ),
            AppError::DecryptFailed(msg) => {
                ("DECRYPT_FAILED".to_string(), format!("解密失败: {}", msg))
            }

            // DLL 错误
            AppError::DllNotFound(path) => (
                "DLL_NOT_FOUND".to_string(),
                format!("DLL 文件不存在: {}", path),
            ),
            AppError::DllLoadFailed(msg) => (
                "DLL_LOAD_FAILED".to_string(),
                format!("DLL 加载失败: {}", msg),
            ),
            AppError::DllFunctionNotInitialized => (
                "DLL_FUNCTION_NOT_INITIALIZED".to_string(),
                "DLL 函数未正确初始化".to_string(),
            ),
            AppError::DllDecodeFailed(code) => (
                "DLL_DECODE_FAILED".to_string(),
                format!("DLL 解码失败,错误代码: {}", code),
            ),
            AppError::WxamDecodeFailed(msg) => (
                "WXAM_DECODE_FAILED".to_string(),
                format!("WXAM 解码失败: {}", msg),
            ),
            AppError::UnsupportedImageFormat(fmt) => (
                "UNSUPPORTED_IMAGE_FORMAT".to_string(),
                format!("不支持的图片格式: {}", fmt),
            ),
            AppError::EmptyInput => ("EMPTY_INPUT".to_string(), "输入数据不能为空".to_string()),
            AppError::InvalidOutputSize => (
                "INVALID_OUTPUT_SIZE".to_string(),
                "输出数据大小无效".to_string(),
            ),

            // 通用错误
            AppError::Internal(msg) => ("INTERNAL_ERROR".to_string(), format!("内部错误: {}", msg)),
        }
    }

    /// 记录错误到日志
    pub fn log(&self) {
        let (code, message) = self.to_code_and_message();
        log::error!("[{}] {}", code, message);
    }
}

/// 将 `AppError` 转换为 `Result` 类型中的错误字符串,用于 Tauri 命令返回
impl From<AppError> for String {
    fn from(err: AppError) -> Self {
        let (code, message) = err.to_code_and_message();
        format!("[{}] {}", code, message)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_code_and_message() {
        let err = AppError::FileNotFound("/path/to/file".to_string());
        let (code, message) = err.to_code_and_message();
        assert_eq!(code, "FILE_NOT_FOUND");
        assert!(message.contains("文件不存在"));
    }

    #[test]
    fn test_error_to_string() {
        let err = AppError::DllNotFound("VoipEngine.dll".to_string());
        let err_str: String = err.into();
        assert!(err_str.contains("DLL_NOT_FOUND"));
        assert!(err_str.contains("VoipEngine.dll"));
    }

    #[test]
    fn test_error_response_from_app_error() {
        let err = AppError::UnsupportedDatVersion;
        let response: ErrorResponse = err.into();
        assert_eq!(response.code, "UNSUPPORTED_DAT_VERSION");
    }
}
