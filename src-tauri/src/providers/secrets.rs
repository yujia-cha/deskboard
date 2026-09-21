//! 토큰을 디스크에 암호화해 둔다 (Windows DPAPI).
//!
//! 기존 `spotify.json` / `claude.json` 은 평문이다. GitHub Personal Access Token 은
//! 조직 저장소 접근 권한까지 가질 수 있어 위험도가 달라서, 새로 저장하는 비밀값은
//! 여기를 거친다.
//!
//! DPAPI 는 **현재 Windows 사용자 계정으로만** 풀 수 있게 묶어준다. 파일을 그대로 복사해
//! 다른 PC·다른 계정에 갖다 놓아도 열리지 않는다. 마스터 비밀번호를 따로 받지 않아도 되는
//! 대신, 그 계정으로 로그인한 프로그램은 풀 수 있다는 한계가 있다.

use std::path::Path;

#[derive(Debug, thiserror::Error)]
pub enum SecretError {
    #[error("{0}")]
    Crypto(String),
    #[error("파일: {0}")]
    Io(#[from] std::io::Error),
}

/// 평문을 암호화한 바이트로.
#[cfg(target_os = "windows")]
pub fn protect(plain: &[u8]) -> Result<Vec<u8>, SecretError> {
    win::crypt(plain, true)
}

/// 암호화된 바이트를 평문으로.
#[cfg(target_os = "windows")]
pub fn unprotect(blob: &[u8]) -> Result<Vec<u8>, SecretError> {
    win::crypt(blob, false)
}

#[cfg(not(target_os = "windows"))]
pub fn protect(_plain: &[u8]) -> Result<Vec<u8>, SecretError> {
    // 평문으로 흘려보내느니 실패하는 편이 낫다.
    Err(SecretError::Crypto("이 플랫폼에서는 암호화 저장을 지원하지 않습니다".into()))
}

#[cfg(not(target_os = "windows"))]
pub fn unprotect(_blob: &[u8]) -> Result<Vec<u8>, SecretError> {
    Err(SecretError::Crypto("이 플랫폼에서는 암호화 저장을 지원하지 않습니다".into()))
}

/// 문자열 비밀값을 파일에 암호화해 쓴다.
pub fn save(path: &Path, secret: &str) -> Result<(), SecretError> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let blob = protect(secret.as_bytes())?;
    std::fs::write(path, blob)?;
    Ok(())
}

/// 파일에서 비밀값을 읽어 푼다. 파일이 없으면 `Ok(None)`.
pub fn load(path: &Path) -> Result<Option<String>, SecretError> {
    if !path.is_file() {
        return Ok(None);
    }
    let blob = std::fs::read(path)?;
    if blob.is_empty() {
        return Ok(None);
    }
    let plain = unprotect(&blob)?;
    String::from_utf8(plain)
        .map(Some)
        .map_err(|_| SecretError::Crypto("저장된 값이 손상됐습니다".into()))
}

/// 비밀값 파일을 지운다. 없으면 아무 일도 하지 않는다.
pub fn clear(path: &Path) -> Result<(), SecretError> {
    if path.is_file() {
        std::fs::remove_file(path)?;
    }
    Ok(())
}

#[cfg(target_os = "windows")]
mod win {
    use super::SecretError;
    use windows_sys::Win32::Foundation::LocalFree;
    use windows_sys::Win32::Security::Cryptography::{
        CryptProtectData, CryptUnprotectData, CRYPT_INTEGER_BLOB,
    };

    /// `encrypt` 가 true 면 보호, false 면 해제. 두 API 의 모양이 같아서 하나로 묶는다.
    pub fn crypt(input: &[u8], encrypt: bool) -> Result<Vec<u8>, SecretError> {
        let mut in_blob = CRYPT_INTEGER_BLOB {
            cbData: input.len() as u32,
            pbData: input.as_ptr() as *mut u8,
        };
        let mut out_blob = CRYPT_INTEGER_BLOB { cbData: 0, pbData: std::ptr::null_mut() };

        // SAFETY: in_blob 은 호출 동안 살아 있는 슬라이스를 가리킨다.
        // 성공하면 out_blob.pbData 는 LocalAlloc 된 버퍼이므로 복사 후 LocalFree 한다.
        let ok = unsafe {
            if encrypt {
                CryptProtectData(
                    &mut in_blob,
                    std::ptr::null(),
                    std::ptr::null_mut(),
                    std::ptr::null_mut(),
                    std::ptr::null_mut(),
                    0,
                    &mut out_blob,
                )
            } else {
                CryptUnprotectData(
                    &mut in_blob,
                    std::ptr::null_mut(),
                    std::ptr::null_mut(),
                    std::ptr::null_mut(),
                    std::ptr::null_mut(),
                    0,
                    &mut out_blob,
                )
            }
        };

        if ok == 0 || out_blob.pbData.is_null() {
            return Err(SecretError::Crypto(if encrypt {
                "토큰을 암호화하지 못했습니다".into()
            } else {
                // 다른 계정·다른 PC 에서 복사해 왔거나 파일이 망가진 경우
                "저장된 토큰을 풀지 못했습니다 — 다시 입력해 주세요".to_string()
            }));
        }

        // SAFETY: 위에서 null 이 아님을 확인했고 cbData 만큼 유효하다.
        let out = unsafe { std::slice::from_raw_parts(out_blob.pbData, out_blob.cbData as usize) }.to_vec();
        unsafe { LocalFree(out_blob.pbData as _) };
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[cfg(target_os = "windows")]
    fn a_secret_survives_a_round_trip() {
        let secret = "ghp_exampleToken1234567890";
        let blob = protect(secret.as_bytes()).unwrap();
        assert_eq!(unprotect(&blob).unwrap(), secret.as_bytes());
    }

    #[test]
    #[cfg(target_os = "windows")]
    fn the_stored_bytes_do_not_contain_the_secret() {
        // 파일을 열어봤을 때 토큰이 그냥 보이면 암호화한 의미가 없다
        let secret = "ghp_averyRecognisableSecret";
        let blob = protect(secret.as_bytes()).unwrap();
        let haystack = String::from_utf8_lossy(&blob);
        assert!(!haystack.contains("ghp_averyRecognisableSecret"));
        assert!(blob.len() > secret.len());
    }

    #[test]
    #[cfg(target_os = "windows")]
    fn a_corrupted_blob_reports_a_readable_error() {
        let mut blob = protect(b"hello").unwrap();
        let n = blob.len();
        blob[n / 2] ^= 0xFF;
        let e = unprotect(&blob).unwrap_err().to_string();
        assert!(e.contains("다시 입력"), "{e}");
    }

    #[test]
    #[cfg(target_os = "windows")]
    fn saving_and_loading_a_file_round_trips() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("token.dat");
        assert_eq!(load(&p).unwrap(), None); // 아직 없음
        save(&p, "secret-value").unwrap();
        assert_eq!(load(&p).unwrap().as_deref(), Some("secret-value"));
        // 파일 내용에 평문이 없어야 한다
        let raw = std::fs::read(&p).unwrap();
        assert!(!String::from_utf8_lossy(&raw).contains("secret-value"));
    }

    #[test]
    #[cfg(target_os = "windows")]
    fn clearing_removes_the_file_and_is_safe_to_repeat() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("token.dat");
        save(&p, "x").unwrap();
        clear(&p).unwrap();
        assert!(!p.exists());
        clear(&p).unwrap(); // 두 번 해도 오류가 아니다
        assert_eq!(load(&p).unwrap(), None);
    }

    #[test]
    #[cfg(target_os = "windows")]
    fn an_empty_file_reads_as_no_secret_rather_than_an_error() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("token.dat");
        std::fs::write(&p, b"").unwrap();
        assert_eq!(load(&p).unwrap(), None);
    }

    #[test]
    #[cfg(target_os = "windows")]
    fn unicode_and_long_secrets_survive() {
        let long = "가".repeat(500);
        let blob = protect(long.as_bytes()).unwrap();
        assert_eq!(String::from_utf8(unprotect(&blob).unwrap()).unwrap(), long);
    }
}
