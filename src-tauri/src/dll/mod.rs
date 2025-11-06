//! DLL 相关模块
//!
//! 包含与 Windows DLL 交互的功能，例如 WXAM 图片解码

pub mod wxam_decoder;

pub use wxam_decoder::{wxam_to_image, ImageFormat, WxAMDecoder};
