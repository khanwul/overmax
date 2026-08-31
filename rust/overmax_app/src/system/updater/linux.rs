use std::path::Path;

use super::version::is_newer_version;
use super::{app_version, main_exe_name, AppUpdateConfig};

const BIN_PATH_IN_ARCHIVE: &str = "overmax/overmax";

fn configure_updater(
    cfg: &AppUpdateConfig,
    current_version: &str,
) -> self_update::backends::github::UpdateBuilder {
    let mut builder = self_update::backends::github::Update::configure();
    builder
        .repo_owner(&cfg.owner)
        .repo_name(&cfg.repo)
        .bin_name(main_exe_name().as_str())
        .bin_path_in_archive(BIN_PATH_IN_ARCHIVE)
        .target("")
        .identifier(&cfg.linux_asset_name)
        .current_version(current_version)
        .no_confirm(true)
        .show_download_progress(false);
    builder
}

pub fn notify_previous_update(_app_dir: &Path) -> Result<bool, String> {
    Ok(true)
}

pub fn check_and_apply_update_blocking(
    _app_dir: &Path,
    cfg: &AppUpdateConfig,
) -> Result<bool, String> {
    if skip_auto_update_by_policy() {
        eprintln!("[AppUpdater] 개발/스킵 모드에서는 자동 패치를 건너뜁니다.");
        return Ok(true);
    }
    if !cfg.enabled {
        return Ok(true);
    }

    let updater = match configure_updater(cfg, app_version()).build() {
        Ok(updater) => updater,
        Err(error) => {
            eprintln!("[AppUpdater] 업데이터 구성 실패: {error}");
            return Ok(true);
        }
    };
    let latest_release = match updater.get_latest_release() {
        Ok(release) => release,
        Err(error) => {
            eprintln!("[AppUpdater] 업데이트 확인 실패: {error}");
            return Ok(true);
        }
    };

    if !is_newer_version(&latest_release.version, app_version()) {
        eprintln!("[AppUpdater] 최신 버전 유지 중: {}", app_version());
        return Ok(true);
    }
    if !ask_update_confirm(app_version(), &latest_release.version) {
        eprintln!("[AppUpdater] 사용자가 이번 실행의 자동 패치를 취소했습니다.");
        return Ok(true);
    }

    eprintln!(
        "[AppUpdater] 새 버전 감지: {} -> {}. 업데이트 진행...",
        app_version(),
        latest_release.version
    );
    let status = match updater.update() {
        Ok(status) => status,
        Err(error) => {
            eprintln!("[AppUpdater] 업데이트 실패: {error}");
            show_update_error(&error.to_string());
            return Ok(true);
        }
    };

    if status.updated() {
        eprintln!("[AppUpdater] 업데이트 완료! 앱을 재시작합니다.");
        Ok(false)
    } else {
        eprintln!("[AppUpdater] 이미 최신 버전입니다.");
        Ok(true)
    }
}

fn ask_update_confirm(current: &str, latest: &str) -> bool {
    rfd::MessageDialog::new()
        .set_title("Overmax Update")
        .set_description(crate::t!(
            "sys-update-prompt-dialog",
            current = current,
            latest = latest
        ))
        .set_level(rfd::MessageLevel::Info)
        .set_buttons(rfd::MessageButtons::YesNo)
        .show()
        == rfd::MessageDialogResult::Yes
}

fn show_update_error(error: &str) {
    let _ = rfd::MessageDialog::new()
        .set_title("Overmax Update Error")
        .set_description(crate::t!("sys-update-error-dialog", error = error))
        .set_level(rfd::MessageLevel::Error)
        .set_buttons(rfd::MessageButtons::Ok)
        .show();
}

fn skip_auto_update_by_policy() -> bool {
    if !super::is_self_update_supported() {
        return true;
    }
    cfg!(debug_assertions)
        || std::env::var("OVERMAX_SKIP_APP_UPDATE")
            .map(|value| value == "1" || value.eq_ignore_ascii_case("true"))
            .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::{configure_updater, AppUpdateConfig};
    use std::fs;
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::os::unix::fs::PermissionsExt;
    use std::path::PathBuf;
    use std::process::Command;

    #[test]
    fn release_tarball_installs_and_updates_without_touching_user_data() {
        let root =
            std::env::temp_dir().join(format!("overmax-linux-update-smoke-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let install = root.join("install/overmax");
        fs::create_dir_all(install.join("cache")).expect("create installed cache");

        let archive = if let Some(path) = std::env::var_os("OVERMAX_LINUX_UPDATE_SMOKE_ARCHIVE") {
            PathBuf::from(path)
        } else {
            let payload = root.join("payload/overmax");
            fs::create_dir_all(&payload).expect("create update payload");
            let payload_bin = payload.join("overmax");
            fs::write(&payload_bin, b"new-linux-binary").expect("write update binary");
            fs::set_permissions(&payload_bin, fs::Permissions::from_mode(0o755))
                .expect("mark update binary executable");
            let archive = root.join("overmax-linux-x86_64.tar.gz");
            assert!(Command::new("tar")
                .args(["-czf"])
                .arg(&archive)
                .args(["-C"])
                .arg(root.join("payload"))
                .arg("overmax")
                .status()
                .expect("run tar")
                .success());
            archive
        };
        let expected = root.join("expected");
        fs::create_dir_all(&expected).expect("create expected payload directory");
        assert!(Command::new("tar")
            .args(["-xzf"])
            .arg(&archive)
            .args(["-C"])
            .arg(&expected)
            .status()
            .expect("extract expected update binary")
            .success());
        let expected_bin =
            fs::read(expected.join("overmax/overmax")).expect("read expected update binary");

        let installed_bin = install.join("overmax");
        fs::write(&installed_bin, b"old-linux-binary").expect("write installed binary");
        fs::write(install.join("settings.user.json"), b"user-settings")
            .expect("write user settings sentinel");
        fs::write(install.join("cache/record.db"), b"user-records").expect("write cache sentinel");

        let listener = TcpListener::bind("127.0.0.1:0").expect("bind update smoke server");
        let address = listener.local_addr().expect("read smoke server address");
        let base_url = format!("http://{address}");
        let archive_bytes = fs::read(&archive).expect("read update archive");
        let release_json = format!(
            r#"[{{"tag_name":"v1.0.1","created_at":"2026-08-11T00:00:00Z","name":"smoke","assets":[{{"name":"overmax-linux-x86_64.tar.gz","url":"{base_url}/asset"}}]}}]"#
        );
        let server = std::thread::spawn(move || {
            for _ in 0..2 {
                let (mut stream, _) = listener.accept().expect("accept smoke request");
                let mut request = [0u8; 4096];
                let read = stream.read(&mut request).expect("read smoke request");
                let request = String::from_utf8_lossy(&request[..read]);
                let (content_type, body): (&str, &[u8]) = if request.starts_with("GET /asset ") {
                    ("application/octet-stream", &archive_bytes)
                } else {
                    ("application/json", release_json.as_bytes())
                };
                write!(
                    stream,
                    "HTTP/1.1 200 OK\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    body.len()
                )
                .expect("write smoke response headers");
                stream.write_all(body).expect("write smoke response body");
            }
        });

        let cfg = AppUpdateConfig::default();
        let mut builder = configure_updater(&cfg, "1.0.0");
        builder.with_url(&base_url).bin_install_path(&installed_bin);
        let status = builder
            .build()
            .expect("build smoke updater")
            .update()
            .expect("apply smoke update");
        assert!(status.updated());
        server.join().expect("join smoke server");

        assert_eq!(fs::read(&installed_bin).unwrap(), expected_bin);
        assert_ne!(
            fs::metadata(&installed_bin).unwrap().permissions().mode() & 0o111,
            0
        );
        assert_eq!(
            fs::read(install.join("settings.user.json")).unwrap(),
            b"user-settings"
        );
        assert_eq!(
            fs::read(install.join("cache/record.db")).unwrap(),
            b"user-records"
        );
        fs::remove_dir_all(root).expect("remove smoke workspace");
    }
}
