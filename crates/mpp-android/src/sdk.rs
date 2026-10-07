use std::{
    env,
    path::{Path, PathBuf},
};

use mpp_core::{Error, Result};

pub(crate) fn discover() -> Result<(PathBuf, Option<PathBuf>)> {
    let mut roots = Vec::new();
    for name in ["ANDROID_HOME", "ANDROID_SDK_ROOT"] {
        if let Some(root) = env::var_os(name).filter(|value| !value.is_empty()) {
            roots.push(PathBuf::from(root));
        }
    }
    if let Some(home) = env::var_os("HOME").filter(|value| !value.is_empty()) {
        let home = PathBuf::from(home);
        if cfg!(target_os = "macos") {
            roots.push(home.join("Library/Android/sdk"));
        } else {
            roots.push(home.join("Android/Sdk"));
        }
    }
    if cfg!(windows)
        && let Some(local) = env::var_os("LOCALAPPDATA")
    {
        roots.push(PathBuf::from(local).join("Android/Sdk"));
    }
    let paths: Vec<_> = env::var_os("PATH")
        .map(|path| env::split_paths(&path).collect())
        .unwrap_or_default();
    let adb =
        resolve(&roots, &paths, "platform-tools", "adb").ok_or_else(|| Error::ToolNotFound {
            tool: "adb (install Android SDK platform-tools; set ANDROID_HOME)".into(),
        })?;
    Ok((adb, resolve(&roots, &paths, "emulator", "emulator")))
}

fn resolve(roots: &[PathBuf], paths: &[PathBuf], directory: &str, tool: &str) -> Option<PathBuf> {
    let file = format!("{tool}{}", env::consts::EXE_SUFFIX);
    roots
        .iter()
        .map(|root| root.join(directory).join(&file))
        .chain(paths.iter().map(|path| path.join(&file)))
        .find(|path| executable(path))
}

fn executable(path: &Path) -> bool {
    let Ok(metadata) = path.metadata() else {
        return false;
    };
    if !metadata.is_file() {
        return false;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if metadata.permissions().mode() & 0o111 == 0 {
            return false;
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sdk_tools_precede_path_without_changing_process_environment() {
        let root = env::temp_dir().join(format!("mpp-sdk-test-{}", std::process::id()));
        let sdk = root.join("sdk");
        let path = root.join("path");
        std::fs::create_dir_all(sdk.join("platform-tools")).unwrap();
        std::fs::create_dir_all(&path).unwrap();
        let name = format!("adb{}", env::consts::EXE_SUFFIX);
        for file in [sdk.join("platform-tools").join(&name), path.join(&name)] {
            std::fs::write(&file, "fake").unwrap();
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                std::fs::set_permissions(file, std::fs::Permissions::from_mode(0o755)).unwrap();
            }
        }
        assert_eq!(
            resolve(
                std::slice::from_ref(&sdk),
                std::slice::from_ref(&path),
                "platform-tools",
                "adb"
            ),
            Some(sdk.join("platform-tools").join(&name))
        );
        assert_eq!(
            resolve(&[], std::slice::from_ref(&path), "platform-tools", "adb"),
            Some(path.join(name))
        );
        std::fs::remove_dir_all(root).unwrap();
    }
}
