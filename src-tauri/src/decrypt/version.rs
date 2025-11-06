//! DAT 文件版本检测模块

use super::error::DecryptError;
use std::fs::File;
use std::io::Read;
use std::path::Path;

/// DAT 文件版本
///
/// 微信 DAT 文件有三种加密版本：
/// - V3: 无签名，仅使用 XOR 加密
/// - V4V1: 带 `\x07\x08V1\x08\x07` 签名，使用固定 AES + XOR 混合加密
/// - V4V2: 带 `\x07\x08V2\x08\x07` 签名，使用动态 AES + XOR 混合加密
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DatVersion {
    /// v3 版本 (仅 XOR 加密，无签名)
    V3,
    /// v4 版本，V1 签名 (固定 AES + XOR 加密)
    V4V1,
    /// v4 版本，V2 签名 (动态 AES + XOR 加密)
    V4V2,
    /// 未知版本
    #[allow(dead_code)]
    Unknown,
}

/// 版本检测器
pub struct VersionDetector;

impl VersionDetector {
    /// v4 V1 签名 (固定 AES)
    pub const V4_V1_SIGNATURE: &'static [u8] = b"\x07\x08V1\x08\x07";
    /// v4 V2 签名 (动态 AES)
    pub const V4_V2_SIGNATURE: &'static [u8] = b"\x07\x08V2\x08\x07";

    /// 检测 DAT 文件版本
    ///
    /// # 参数
    ///
    /// * `input_path` - 输入文件路径
    ///
    /// # 返回
    ///
    /// 返回检测到的 DAT 版本
    pub fn detect<P: AsRef<Path>>(input_path: P) -> Result<DatVersion, DecryptError> {
        let mut file = File::open(input_path)?;
        let mut signature = [0u8; 6];

        // 尝试读取签名，如果失败或不匹配，则为 V3 (无签名)
        if file.read_exact(&mut signature).is_err() {
            return Ok(DatVersion::V3);
        }

        match &signature {
            s if s == Self::V4_V1_SIGNATURE => Ok(DatVersion::V4V1),
            s if s == Self::V4_V2_SIGNATURE => Ok(DatVersion::V4V2),
            _ => Ok(DatVersion::V3), // 无签名视为 V3
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_signatures() {
        assert_eq!(VersionDetector::V4_V1_SIGNATURE, b"\x07\x08V1\x08\x07");
        assert_eq!(VersionDetector::V4_V2_SIGNATURE, b"\x07\x08V2\x08\x07");
    }
}
