//! Read most-recent Steam login id from `loginusers.vdf` (same logic as Python `steam_session.py`).

use std::collections::HashMap;
use std::fs;
use std::path::Path;

use winreg::enums::{HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE};
use winreg::RegKey;

pub struct SteamUser {
    pub steam_id: String,
    pub persona_name: String,
    pub account_name: String,
}

pub fn all_login_users() -> Vec<SteamUser> {
    let mut results = Vec::new();
    let Some(vdf) = read_loginusers_vdf() else {
        return results;
    };
    let data = parse_vdf(&vdf);
    let users = match data.get("users") {
        Some(VdfVal::Obj(m)) => m,
        _ => return results,
    };
    for (steam_id, user_data) in users {
        let attrs = match user_data {
            VdfVal::Obj(m) => m,
            _ => continue,
        };
        let persona_name = if let Some(VdfVal::Str(s)) = attrs.get("personaname") {
            s.clone()
        } else {
            String::new()
        };
        let account_name = if let Some(VdfVal::Str(s)) = attrs.get("accountname") {
            s.clone()
        } else {
            String::new()
        };
        results.push(SteamUser {
            steam_id: steam_id.clone(),
            persona_name,
            account_name,
        });
    }
    results
}

pub fn most_recent_steam_id() -> Option<String> {
    let vdf = read_loginusers_vdf()?;
    let data = parse_vdf(&vdf);
    let users = match data.get("users")? {
        VdfVal::Obj(m) => m,
        _ => return None,
    };
    for (steam_id, user_data) in users {
        let attrs = match user_data {
            VdfVal::Obj(m) => m,
            _ => continue,
        };
        if let Some(VdfVal::Str(s)) = attrs.get("mostrecent") {
            if s == "1" {
                return Some(steam_id.clone());
            }
        }
    }
    None
}

fn read_loginusers_vdf() -> Option<String> {
    let steam = find_steam_path()?;
    let path = Path::new(&steam).join("config").join("loginusers.vdf");
    fs::read_to_string(path).ok()
}

fn find_steam_path() -> Option<String> {
    // 1. 하드코딩된 기본 경로 탐색
    for path in [r"C:\Program Files (x86)\Steam", r"C:\Program Files\Steam"] {
        if Path::new(path).exists() {
            return Some(path.to_string());
        }
    }

    // 2. HKCU 레지스트리 탐색
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    if let Ok(key) = hkcu.open_subkey(r"Software\Valve\Steam") {
        if let Ok(steam_path) = key.get_value::<String, _>("SteamPath") {
            let trimmed = steam_path
                .trim()
                .trim_end_matches('/')
                .trim_end_matches('\\');
            if !trimmed.is_empty() {
                return Some(trimmed.to_string());
            }
        }
    }

    // 3. HKLM 레지스트리 탐색
    let hklm = RegKey::predef(HKEY_LOCAL_MACHINE);
    for subkey in [r"Software\Valve\Steam", r"Software\Wow6432Node\Valve\Steam"] {
        if let Ok(key) = hklm.open_subkey(subkey) {
            for val_name in ["SteamPath", "InstallPath"] {
                if let Ok(steam_path) = key.get_value::<String, _>(val_name) {
                    let trimmed = steam_path
                        .trim()
                        .trim_end_matches('/')
                        .trim_end_matches('\\');
                    if !trimmed.is_empty() {
                        return Some(trimmed.to_string());
                    }
                }
            }
        }
    }

    // 4. 실행 중인 프로세스 목록에서 탐색
    if let Some(path) = find_steam_from_processes() {
        return Some(path);
    }

    None
}

#[cfg(windows)]
fn find_steam_from_processes() -> Option<String> {
    use std::ffi::OsString;
    use std::os::windows::ffi::OsStringExt;
    use std::path::PathBuf;
    use windows_sys::Win32::Foundation::{CloseHandle, INVALID_HANDLE_VALUE};
    use windows_sys::Win32::System::Diagnostics::ToolHelp::{
        CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W, TH32CS_SNAPPROCESS,
    };
    use windows_sys::Win32::System::Threading::{
        OpenProcess, QueryFullProcessImageNameW, PROCESS_QUERY_LIMITED_INFORMATION,
    };

    unsafe {
        let snapshot = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0);
        if snapshot == INVALID_HANDLE_VALUE {
            return None;
        }

        let mut entry: PROCESSENTRY32W = std::mem::zeroed();
        entry.dwSize = std::mem::size_of::<PROCESSENTRY32W>() as u32;

        if Process32FirstW(snapshot, &mut entry) != 0 {
            loop {
                let len = entry
                    .szExeFile
                    .iter()
                    .position(|&c| c == 0)
                    .unwrap_or(entry.szExeFile.len());
                let exe_name = OsString::from_wide(&entry.szExeFile[..len]);
                if exe_name.to_string_lossy().eq_ignore_ascii_case("steam.exe") {
                    let pid = entry.th32ProcessID;
                    let handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
                    if !handle.is_null() {
                        let mut buffer = [0u16; 4096];
                        let mut size = buffer.len() as u32;
                        if QueryFullProcessImageNameW(handle, 0, buffer.as_mut_ptr(), &mut size) != 0 {
                            CloseHandle(handle);
                            CloseHandle(snapshot);
                            let full_path = OsString::from_wide(&buffer[..size as usize]);
                            let path = PathBuf::from(full_path);
                            if let Some(parent) = path.parent() {
                                return Some(parent.to_string_lossy().into_owned());
                            }
                            return None;
                        }
                        CloseHandle(handle);
                    }
                }

                if Process32NextW(snapshot, &mut entry) == 0 {
                    break;
                }
            }
        }
        CloseHandle(snapshot);
    }
    None
}

#[cfg(not(windows))]
fn find_steam_from_processes() -> Option<String> {
    None
}

#[derive(Debug, Clone)]
enum VdfVal {
    Str(String),
    Obj(HashMap<String, VdfVal>),
}

/// Minimal VDF parser (mirrors Python `parse_vdf` in `steam_session.py`).
fn parse_vdf(content: &str) -> HashMap<String, VdfVal> {
    let mut map_stack: Vec<HashMap<String, VdfVal>> = vec![HashMap::new()];
    let mut key_stack: Vec<String> = Vec::new();
    let mut pending_key: Option<String> = None;

    for raw in content.lines() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with("//") || line.starts_with('#') {
            continue;
        }
        if line == "{" {
            if let Some(k) = pending_key.take() {
                key_stack.push(k.to_lowercase());
                map_stack.push(HashMap::new());
            }
            continue;
        }
        if line == "}" {
            if map_stack.len() > 1 {
                if let (Some(done), Some(key)) = (map_stack.pop(), key_stack.pop()) {
                    if let Some(parent) = map_stack.last_mut() {
                        parent.insert(key, VdfVal::Obj(done));
                    }
                }
            }
            continue;
        }
        if let Some((k, v)) = parse_quoted_key_value_line(line) {
            if let Some(parent) = map_stack.last_mut() {
                parent.insert(k.to_lowercase(), VdfVal::Str(v));
            }
            pending_key = None;
        } else if let Some(k) = parse_quoted_key_open_brace(line) {
            key_stack.push(k.to_lowercase());
            map_stack.push(HashMap::new());
            pending_key = None;
        } else if let Some(k) = parse_quoted_key_only_line(line) {
            pending_key = Some(k.to_lowercase());
        }
    }
    map_stack.into_iter().next().unwrap_or_default()
}

fn parse_quoted_key_value_line(line: &str) -> Option<(String, String)> {
    let s = line.trim();
    let rest = s.strip_prefix('"')?;
    let (key, rest) = take_until_quote(rest)?;
    let rest = rest.trim();
    let rest = rest.strip_prefix('"')?;
    let (value, tail) = take_until_quote(rest)?;
    if !tail.trim().is_empty() {
        return None;
    }
    Some((key, value))
}

fn parse_quoted_key_only_line(line: &str) -> Option<String> {
    let s = line.trim();
    if s.contains('{') {
        return None;
    }
    let rest = s.strip_prefix('"')?;
    let (key, tail) = take_until_quote(rest)?;
    if !tail.trim().is_empty() {
        return None;
    }
    Some(key)
}

fn parse_quoted_key_open_brace(line: &str) -> Option<String> {
    let s = line.trim();
    if !s.contains('{') {
        return None;
    }
    let before = s.split('{').next()?.trim();
    let rest = before.strip_prefix('"')?;
    let (key, tail) = take_until_quote(rest)?;
    if !tail.trim().is_empty() {
        return None;
    }
    Some(key)
}

fn take_until_quote(s: &str) -> Option<(String, &str)> {
    let mut i = 0;
    let bytes = s.as_bytes();
    while i < bytes.len() {
        if bytes[i] == b'"' {
            return Some((s[..i].to_string(), &s[i + 1..]));
        }
        i += 1;
    }
    None
}

#[cfg(test)]
mod tests {
    use super::{parse_vdf, VdfVal};

    #[test]
    fn parse_sample_loginusers() {
        let sample = r#""users"
{
    "76561198000000001"
    {
        "AccountName" "test"
        "MostRecent" "0"
    }
    "76561198000000002"
    {
        "AccountName" "main"
        "MostRecent" "1"
    }
}
"#;
        let m = parse_vdf(sample);
        let users = match m.get("users") {
            Some(VdfVal::Obj(u)) => u,
            _ => panic!("users"),
        };
        let u2 = match users.get("76561198000000002") {
            Some(VdfVal::Obj(b)) => b,
            _ => panic!("user2"),
        };
        assert!(matches!(
            u2.get("mostrecent"),
            Some(VdfVal::Str(s)) if s == "1"
        ));
    }
}
