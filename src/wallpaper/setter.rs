use anyhow::{anyhow, Result};
use std::path::Path;
use std::process::Command;

#[derive(Debug, Clone, PartialEq)]
pub enum WallpaperBackend {
    /// Hyprland + hyprpaper: hyprctl keyword wallpaper
    Hyprpaper,
    /// Sway 等の Wayland: swaybg プロセスを再起動
    Swaybg,
    /// GNOME (X11/Wayland): gsettings
    Gsettings,
    /// X11 汎用: feh
    Feh,
    /// macOS: osascript
    Macos,
    /// Windows: SystemParametersInfoW
    Windows,
}

pub struct WallpaperSetter {
    backend: WallpaperBackend,
}

impl WallpaperSetter {
    pub fn new(backend_str: &str) -> Result<Self> {
        let backend = match backend_str {
            "hyprpaper" => WallpaperBackend::Hyprpaper,
            "swaybg" => WallpaperBackend::Swaybg,
            "gsettings" => WallpaperBackend::Gsettings,
            "feh" => WallpaperBackend::Feh,
            "macos" => WallpaperBackend::Macos,
            "windows" => WallpaperBackend::Windows,
            "auto" | _ => detect_backend()?,
        };
        tracing::info!("壁紙バックエンド: {:?}", backend);
        Ok(Self { backend })
    }

    pub fn set(&self, path: &Path) -> Result<()> {
        match &self.backend {
            WallpaperBackend::Hyprpaper => self.set_wallpaper_hyprpaper(path),
            WallpaperBackend::Swaybg => set_wallpaper_swaybg(path),
            WallpaperBackend::Gsettings => set_wallpaper_gsettings(path),
            WallpaperBackend::Feh => set_wallpaper_feh(path),
            WallpaperBackend::Macos => set_wallpaper_macos(path),
            WallpaperBackend::Windows => set_wallpaper_windows(path),
        }
    }
}

fn detect_backend() -> Result<WallpaperBackend> {
    #[cfg(target_os = "macos")]
    return Ok(WallpaperBackend::Macos);

    #[cfg(target_os = "windows")]
    return Ok(WallpaperBackend::Windows);

    #[cfg(target_os = "linux")]
    {
        // Hyprland チェック (最優先: HYPRLAND_INSTANCE_SIGNATURE が設定されている)
        if std::env::var("HYPRLAND_INSTANCE_SIGNATURE").is_ok() && command_exists("hyprctl") {
            return Ok(WallpaperBackend::Hyprpaper);
        }

        // Wayland チェック
        if std::env::var("WAYLAND_DISPLAY").is_ok() {
            // Sway
            if std::env::var("SWAYSOCK").is_ok() {
                return Ok(WallpaperBackend::Swaybg);
            }
            // GNOME Wayland
            if let Ok(desktop) = std::env::var("XDG_CURRENT_DESKTOP") {
                let d = desktop.to_lowercase();
                if d.contains("gnome") || d.contains("unity") {
                    if command_exists("gsettings") {
                        return Ok(WallpaperBackend::Gsettings);
                    }
                }
            }
            if command_exists("swaybg") {
                return Ok(WallpaperBackend::Swaybg);
            }
        }

        // X11 チェック
        if std::env::var("DISPLAY").is_ok() {
            if let Ok(desktop) = std::env::var("XDG_CURRENT_DESKTOP") {
                let d = desktop.to_lowercase();
                if d.contains("gnome") || d.contains("unity") {
                    if command_exists("gsettings") {
                        return Ok(WallpaperBackend::Gsettings);
                    }
                }
            }
            if command_exists("feh") {
                return Ok(WallpaperBackend::Feh);
            }
        }

        Err(anyhow!(
            "壁紙バックエンドを自動検出できません。\n\
            --backend オプションで hyprpaper / feh / swaybg / gsettings を指定してください。"
        ))
    }

    #[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
    Err(anyhow!("未対応のプラットフォームです"))
}

fn command_exists(cmd: &str) -> bool {
    Command::new("which")
        .arg(cmd)
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

impl WallpaperSetter {
    /// Hyprland + hyprpaper: conf を書き換えて hyprpaper を再起動
    fn set_wallpaper_hyprpaper(&self, path: &Path) -> Result<()> {
        let canonical = path.canonicalize()?;
        let path_str = canonical.to_string_lossy().to_string();

        let monitors = get_hyprland_monitors()?;
        if monitors.is_empty() {
            return Err(anyhow!("Hyprland モニターを取得できません"));
        }

        // hyprpaper v0.8.3 以降のブロック形式
        let conf_path = find_hyprpaper_conf()?;
        let mut new_conf = String::new();
        for monitor in &monitors {
            new_conf.push_str(&format!(
                "wallpaper {{\n  monitor = {}\n  path = {}\n  fit_mode = cover\n}}\n",
                monitor, path_str
            ));
        }
        if let Some(parent) = conf_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&conf_path, &new_conf)?;

        // 既存の hyprpaper を終了して再起動
        let _ = Command::new("pkill").args(["-x", "hyprpaper"]).status();
        std::thread::sleep(std::time::Duration::from_millis(100));
        Command::new("hyprpaper")
            .spawn()
            .map_err(|e| anyhow!("hyprpaper の起動に失敗: {}", e))?;
        std::thread::sleep(std::time::Duration::from_millis(400));

        tracing::debug!("hyprpaper 壁紙設定: {}", path_str);
        Ok(())
    }
}

/// 実行中の Hyprland/hyprpaper プロセスから HYPRLAND_INSTANCE_SIGNATURE を取得
fn find_hyprland_signature_from_proc() -> Option<String> {
    // /proc/<pid>/environ を探す対象プロセス名
    for name in &["Hyprland", "hyprpaper"] {
        if let Ok(output) = Command::new("pgrep").args(["-x", name]).output() {
            let pids = String::from_utf8_lossy(&output.stdout);
            for pid in pids.split_whitespace() {
                let env_path = format!("/proc/{}/environ", pid.trim());
                if let Ok(data) = std::fs::read(&env_path) {
                    for entry in data.split(|&b| b == 0) {
                        if let Ok(s) = std::str::from_utf8(entry) {
                            if let Some(val) = s.strip_prefix("HYPRLAND_INSTANCE_SIGNATURE=") {
                                return Some(val.to_string());
                            }
                        }
                    }
                }
            }
        }
    }
    None
}

/// hyprpaper.conf のパスを返す
fn find_hyprpaper_conf() -> Result<std::path::PathBuf> {
    // XDG_CONFIG_HOME or ~/.config/hypr/hyprpaper.conf
    let xdg_conf = std::env::var("XDG_CONFIG_HOME")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|_| {
            dirs_home().join(".config")
        });

    let candidates = [
        xdg_conf.join("hypr").join("hyprpaper.conf"),
        dirs_home().join(".config").join("hypr").join("hyprpaper.conf"),
    ];

    for p in &candidates {
        if p.exists() {
            return Ok(p.clone());
        }
    }

    // 存在しなければ最初の候補パスを返す (新規作成)
    Ok(candidates[0].clone())
}

fn dirs_home() -> std::path::PathBuf {
    std::env::var("HOME")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|_| std::path::PathBuf::from("/root"))
}

/// `hyprctl monitors -j` でモニター情報を取得
fn get_hyprland_monitors() -> Result<Vec<String>> {
    let monitors = get_hyprland_monitor_info()?;
    Ok(monitors.into_iter().map(|(name, _, _, _)| name).collect())
}

/// モニターの (name, width, height, scale) を取得
pub fn get_hyprland_monitor_info() -> Result<Vec<(String, u32, u32, f32)>> {
    // HYPRLAND_INSTANCE_SIGNATURE が未設定なら実行中プロセスから補完
    if std::env::var("HYPRLAND_INSTANCE_SIGNATURE").is_err() {
        if let Some(sig) = find_hyprland_signature_from_proc() {
            // 環境変数を設定してから実行 (本プロセスのみに影響)
            std::env::set_var("HYPRLAND_INSTANCE_SIGNATURE", &sig);
            tracing::debug!("HYPRLAND_INSTANCE_SIGNATURE をプロセスから取得: {}", sig);
        }
    }

    let output = Command::new("hyprctl")
        .args(["monitors", "-j"])
        .output()?;

    if !output.status.success() {
        return Err(anyhow!(
            "hyprctl monitors の実行に失敗しました: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }

    let json_str = String::from_utf8_lossy(&output.stdout);

    // 簡易 JSON パース: name, width, height, scale を抽出
    // 形式: [{"name":"HDMI-A-1","width":3840,"height":2160,"scale":1.5,...},...]
    let mut monitors = Vec::new();
    let text = json_str.as_ref();

    // 各オブジェクトを { ... } で分割
    let mut depth = 0i32;
    let mut obj_start = None;

    for (i, ch) in text.char_indices() {
        match ch {
            '{' => {
                depth += 1;
                if depth == 1 {
                    obj_start = Some(i);
                }
            }
            '}' => {
                depth -= 1;
                if depth == 0 {
                    if let Some(start) = obj_start {
                        let obj = &text[start..=i];
                        if let Some(info) = parse_monitor_object(obj) {
                            monitors.push(info);
                        }
                    }
                    obj_start = None;
                }
            }
            _ => {}
        }
    }

    if monitors.is_empty() {
        return Err(anyhow!("モニター情報を解析できませんでした"));
    }

    tracing::debug!("検出されたモニター: {:?}", monitors);
    Ok(monitors)
}

fn parse_monitor_object(obj: &str) -> Option<(String, u32, u32, f32)> {
    let name = extract_json_str(obj, "name")?;
    let width = extract_json_num(obj, "\"width\":")?;
    let height = extract_json_num(obj, "\"height\":")?;
    let scale = extract_json_float(obj, "\"scale\":")?;
    Some((name, width as u32, height as u32, scale))
}

fn extract_json_str(obj: &str, key: &str) -> Option<String> {
    let search = format!("\"{}\":", key);
    let pos = obj.find(&search)?;
    let rest = &obj[pos + search.len()..];
    let start = rest.find('"')? + 1;
    let end = rest[start..].find('"')?;
    Some(rest[start..start + end].to_string())
}

fn extract_json_num(obj: &str, key: &str) -> Option<f64> {
    let pos = obj.find(key)?;
    let rest = &obj[pos + key.len()..].trim_start();
    let end = rest.find(|c: char| !c.is_ascii_digit() && c != '.' && c != '-')?;
    rest[..end].parse().ok()
}

fn extract_json_float(obj: &str, key: &str) -> Option<f32> {
    extract_json_num(obj, key).map(|v| v as f32)
}

fn set_wallpaper_feh(path: &Path) -> Result<()> {
    let status = Command::new("feh")
        .args(["--bg-fill", &path.to_string_lossy()])
        .status()?;
    if !status.success() {
        return Err(anyhow!("feh の実行に失敗しました"));
    }
    Ok(())
}

fn set_wallpaper_swaybg(path: &Path) -> Result<()> {
    // 既存の swaybg を終了
    let _ = Command::new("pkill").args(["-x", "swaybg"]).status();

    // 新しい swaybg を起動
    Command::new("swaybg")
        .args(["-i", &path.to_string_lossy(), "-m", "fill"])
        .spawn()?;

    std::thread::sleep(std::time::Duration::from_millis(200));
    Ok(())
}

fn set_wallpaper_gsettings(path: &Path) -> Result<()> {
    let canonical = path.canonicalize()?;
    let uri = format!("file://{}", canonical.display());

    let status = Command::new("gsettings")
        .args([
            "set",
            "org.gnome.desktop.background",
            "picture-uri",
            &uri,
        ])
        .status()?;
    if !status.success() {
        return Err(anyhow!("gsettings の実行に失敗しました"));
    }

    let _ = Command::new("gsettings")
        .args([
            "set",
            "org.gnome.desktop.background",
            "picture-uri-dark",
            &uri,
        ])
        .status();

    Ok(())
}

fn set_wallpaper_macos(path: &Path) -> Result<()> {
    let canonical = path.canonicalize()?;
    let path_str = canonical.to_string_lossy();
    let script = format!(
        r#"tell application "System Events"
            tell every desktop
                set picture to "{}"
            end tell
        end tell"#,
        path_str
    );

    let status = Command::new("osascript").args(["-e", &script]).status()?;
    if !status.success() {
        return Err(anyhow!("osascript の実行に失敗しました"));
    }
    Ok(())
}

#[cfg(target_os = "windows")]
fn set_wallpaper_windows(path: &Path) -> Result<()> {
    use windows::Win32::UI::WindowsAndMessaging::{
        SystemParametersInfoW, SPI_SETDESKWALLPAPER, SPIF_SENDCHANGE, SPIF_UPDATEINIFILE,
    };

    let canonical = path.canonicalize()?;
    let path_wide: Vec<u16> = canonical
        .to_string_lossy()
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect();

    unsafe {
        SystemParametersInfoW(
            SPI_SETDESKWALLPAPER,
            0,
            Some(path_wide.as_ptr() as *mut _),
            SPIF_SENDCHANGE | SPIF_UPDATEINIFILE,
        )?;
    }
    Ok(())
}

#[cfg(not(target_os = "windows"))]
fn set_wallpaper_windows(_path: &Path) -> Result<()> {
    Err(anyhow!("Windows API は Windows でのみ使用できます"))
}
