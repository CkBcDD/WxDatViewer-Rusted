//! 微信 WXAM 文件解码模块
//!
//! 该模块提供了将微信 WXAM 格式文件转换为标准图片格式(JPEG/GIF)的功能。

use std::ffi::c_void;
use std::path::PathBuf;
use std::sync::OnceLock;
use thiserror::Error;
use windows::core::PCWSTR;
use windows::Win32::Foundation::HMODULE;
use windows::Win32::System::LibraryLoader::{FreeLibrary, LoadLibraryW};

/// 支持的图片格式
#[repr(i32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImageFormat {
    /// JPEG 格式
    Jpeg = 0,
    /// GIF 格式
    Gif = 3,
}

/// WXAM 解码配置结构体
#[repr(C)]
struct WxAMConfig {
    /// 解码模式
    mode: i32,
    /// 保留字段
    reserved: i32,
}

/// WXAM 解码错误类型
#[derive(Error, Debug)]
pub enum WxAMError {
    #[error("DLL 文件不存在: {0}")]
    DllNotFound(String),

    #[error("DLL 加载失败: {0}")]
    DllLoadFailed(String),

    #[error("DLL 函数未正确初始化")]
    FunctionNotInitialized,

    #[error("输入数据不能为空")]
    EmptyInput,

    #[error("不支持的格式: {0:?}")]
    UnsupportedFormat(i32),

    #[error("DLL 解码失败,错误代码: {0}")]
    DecodeFailed(i64),

    #[error("解码结果大小无效")]
    InvalidOutputSize,

    #[error("解码过程异常: {0}")]
    DecodeError(String),
}

/// DLL 函数指针类型
type WxamDecFunction = unsafe extern "system" fn(
    input_addr: i64,
    input_size: i32,
    output_addr: i64,
    output_size_ptr: *mut i32,
    config_addr: i64,
) -> i64;

/// DLL 实例持有者
struct DllHolder {
    _handle: HMODULE,
    function: WxamDecFunction,
}

impl Drop for DllHolder {
    fn drop(&mut self) {
        unsafe {
            let _ = FreeLibrary(self._handle);
        }
    }
}

// 全局 DLL 实例
static DLL_INSTANCE: OnceLock<Result<DllHolder, String>> = OnceLock::new();

/// WXAM 格式解码器
///
/// 负责加载 DLL 并提供 WXAM 到图片格式的转换功能。
pub struct WxAMDecoder;

impl WxAMDecoder {
    /// 最大输出大小 (52MB)
    const MAX_OUTPUT_SIZE: usize = 52 * 1024 * 1024;

    /// DLL 文件名
    const DLL_NAME: &'static str = "VoipEngine.dll";

    /// 加载 VoipEngine.dll
    fn load_dll() -> Result<&'static DllHolder, WxAMError> {
        DLL_INSTANCE
            .get_or_init(|| Self::load_dll_internal().map_err(|e| e.to_string()))
            .as_ref()
            .map_err(|e| WxAMError::DllLoadFailed(e.clone()))
    }

    /// 内部 DLL 加载实现
    fn load_dll_internal() -> Result<DllHolder, WxAMError> {
        // 获取 DLL 路径
        let dll_path = std::env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(|p| p.to_path_buf()))
            .unwrap_or_else(|| PathBuf::from("."))
            .join(Self::DLL_NAME);

        if !dll_path.exists() {
            return Err(WxAMError::DllNotFound(dll_path.display().to_string()));
        }

        // 转换为 UTF-16
        let dll_path_wide: Vec<u16> = dll_path
            .to_string_lossy()
            .encode_utf16()
            .chain(std::iter::once(0))
            .collect();

        // 加载 DLL
        let handle = unsafe {
            LoadLibraryW(PCWSTR::from_raw(dll_path_wide.as_ptr()))
                .map_err(|e| WxAMError::DllLoadFailed(format!("LoadLibrary 失败: {}", e)))?
        };

        // 获取函数地址
        let func_name = b"wxam_dec_wxam2pic_5\0";
        let func_ptr = unsafe {
            windows::Win32::System::LibraryLoader::GetProcAddress(
                handle,
                windows::core::PCSTR::from_raw(func_name.as_ptr()),
            )
            .ok_or_else(|| {
                WxAMError::DllLoadFailed("无法找到函数 wxam_dec_wxam2pic_5".to_string())
            })?
        };

        let function: WxamDecFunction = unsafe { std::mem::transmute(func_ptr) };

        log::info!("成功加载 {}", Self::DLL_NAME);

        Ok(DllHolder {
            _handle: handle,
            function,
        })
    }

    /// 将 WXAM 格式数据转换为图片格式
    ///
    /// # 参数
    ///
    /// * `data` - WXAM 格式的原始字节数据
    /// * `format` - 目标图片格式,默认为 JPEG
    ///
    /// # 返回
    ///
    /// 转换后的图片字节数据
    ///
    /// # 错误
    ///
    /// 当参数验证失败或解码失败时返回错误
    pub fn decode(data: &[u8], format: ImageFormat) -> Result<Vec<u8>, WxAMError> {
        // 验证 DLL 是否已加载
        let dll_holder = Self::load_dll()?;

        // 参数验证
        if data.is_empty() {
            return Err(WxAMError::EmptyInput);
        }

        // 创建配置结构体
        let config = WxAMConfig {
            mode: format as i32,
            reserved: 0,
        };

        // 准备输出缓冲区
        let mut output_buffer = vec![0u8; Self::MAX_OUTPUT_SIZE];
        let mut output_size = Self::MAX_OUTPUT_SIZE as i32;

        log::debug!(
            "开始解码 WXAM 数据,大小: {} 字节,格式: {:?}",
            data.len(),
            format
        );

        // 调用 DLL 函数
        let result = unsafe {
            (dll_holder.function)(
                data.as_ptr() as i64,
                data.len() as i32,
                output_buffer.as_mut_ptr() as i64,
                &mut output_size as *mut i32,
                &config as *const WxAMConfig as i64,
            )
        };

        // 检查返回值
        if result != 0 {
            return Err(WxAMError::DecodeFailed(result));
        }

        if output_size <= 0 {
            return Err(WxAMError::InvalidOutputSize);
        }

        // 截取有效数据
        output_buffer.truncate(output_size as usize);

        log::debug!("解码成功,输出大小: {} 字节", output_size);

        Ok(output_buffer)
    }
}

/// 便捷函数: 将 WXAM 格式数据转换为图片格式
///
/// # 参数
///
/// * `data` - WXAM 格式的原始字节数据
/// * `format` - 目标图片格式字符串 ("jpeg" 或 "gif"),默认为 "jpeg"
///
/// # 返回
///
/// 转换后的图片字节数据,失败时返回 None
pub fn wxam_to_image(data: &[u8], format: &str) -> Option<Vec<u8>> {
    let image_format = match format.to_lowercase().as_str() {
        "jpeg" => ImageFormat::Jpeg,
        "gif" => ImageFormat::Gif,
        _ => {
            log::error!("不支持的格式: {}", format);
            return None;
        }
    };

    match WxAMDecoder::decode(data, image_format) {
        Ok(result) => Some(result),
        Err(e) => {
            log::error!("解码失败: {}", e);
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_empty_input() {
        let result = WxAMDecoder::decode(&[], ImageFormat::Jpeg);
        assert!(matches!(result, Err(WxAMError::EmptyInput)));
    }

    #[test]
    fn test_format_conversion() {
        assert_eq!(ImageFormat::Jpeg as i32, 0);
        assert_eq!(ImageFormat::Gif as i32, 3);
    }
}
