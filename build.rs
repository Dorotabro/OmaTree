use cxx_qt_build::{CxxQtBuilder, QResource, QResources, QmlFile, QmlModule};

fn main() {
    CxxQtBuilder::new_qml_module(QmlModule::new("org.omatree").qml_files([
        QmlFile::from("qml/Ui.qml").singleton(true),
        QmlFile::from("qml/Keymap.qml").singleton(true),
        QmlFile::from("qml/main.qml"),
        QmlFile::from("qml/TreePane.qml"),
        QmlFile::from("qml/EditorPane.qml"),
        QmlFile::from("qml/RecoveryDialog.qml"),
        QmlFile::from("qml/ThemedDialog.qml"),
        QmlFile::from("qml/SearchPane.qml"),
        QmlFile::from("qml/SearchField.qml"),
        QmlFile::from("qml/Command.qml"),
        QmlFile::from("qml/PointerHover.qml"),
        QmlFile::from("qml/KeyboardShortcuts.qml"),
    ]))
    .qt_module("Quick")
    .files([
        "src/app.rs",
        "src/tree_model.rs",
        "src/theme_model.rs",
        "src/markdown.rs",
    ])
    .cpp_file("cpp/markdown_render.cpp")
    .cpp_file("cpp/app_identity.cpp")
    .cpp_file("cpp/controls_style.cpp")
    .cpp_file("cpp/keyboard_policy.cpp")
    .cpp_file("cpp/file_open.cpp")
    // The application icon, compiled into the executable (see
    // cpp/app_identity.cpp), so the window has it however OmaTree is started.
    .qrc_resources(
        QResources::new().resource(QResource::new().prefix("/icons").files([
            "assets/logo/omatree-icon-32.png",
            "assets/logo/omatree-icon-64.png",
            "assets/logo/omatree-icon-128.png",
            "assets/logo/omatree-icon-256.png",
            "assets/logo/omatree-icon-512.png",
        ])),
    )
    .build();

    embed_windows_resources();
}

/// Windows (MSVC) only: embeds the executable's resources, defined in
/// `windows/omatree.rc`: the icon, the version information (taken from
/// Cargo.toml) and the manifest `windows/omatree.manifest`, which makes the
/// process's ANSI code page UTF-8 so that a non-ASCII command-line path reaches
/// Qt intact. They go into the executable only (not the test programs), through
/// this one path, so there is exactly one manifest resource.
fn embed_windows_resources() {
    use std::path::{Path, PathBuf};

    for file in ["omatree.rc", "omatree.manifest", "omatree.ico"] {
        println!("cargo:rerun-if-changed=windows/{file}");
    }
    let var = |name: &str| std::env::var(name).unwrap_or_default();
    if var("CARGO_CFG_TARGET_OS") != "windows" || var("CARGO_CFG_TARGET_ENV") != "msvc" {
        return;
    }
    let windows_dir = Path::new(&var("CARGO_MANIFEST_DIR")).join("windows");
    let out_dir = PathBuf::from(var("OUT_DIR"));

    // The version, as the two forms a version resource needs: "0.1.1" and
    // 0,1,1,0 (a numeric part without a pre-release suffix).
    let number = |name: &str| var(name).parse::<u16>().unwrap_or(0);
    let header = format!(
        "#define OMATREE_VERSION_STR \"{}\"\n#define OMATREE_VERSION_NUM {},{},{},0\n",
        var("CARGO_PKG_VERSION"),
        number("CARGO_PKG_VERSION_MAJOR"),
        number("CARGO_PKG_VERSION_MINOR"),
        number("CARGO_PKG_VERSION_PATCH"),
    );
    std::fs::write(out_dir.join("version.h"), header).expect("could not write version.h");

    let res = out_dir.join("omatree.res");
    let status = std::process::Command::new(find_rc())
        .arg("/nologo")
        .arg("/I")
        .arg(&windows_dir)
        .arg("/I")
        .arg(&out_dir)
        .arg("/fo")
        .arg(&res)
        .arg(windows_dir.join("omatree.rc"))
        .status()
        .expect("could not run rc.exe");
    assert!(status.success(), "rc.exe failed on windows/omatree.rc");
    println!("cargo:rustc-link-arg-bins={}", res.display());
}

/// The Windows SDK's resource compiler: `RC` if set, else `rc.exe` on `PATH`
/// (a Visual Studio developer shell has it), else the newest in the SDK's
/// default location. Every MSVC setup that can link has the SDK.
fn find_rc() -> std::path::PathBuf {
    use std::path::PathBuf;

    if let Some(rc) = std::env::var_os("RC") {
        return PathBuf::from(rc);
    }
    if let Some(path) = std::env::var_os("PATH") {
        for dir in std::env::split_paths(&path) {
            let candidate = dir.join("rc.exe");
            if candidate.is_file() {
                return candidate;
            }
        }
    }
    let program_files = std::env::var_os("ProgramFiles(x86)").unwrap_or_default();
    let bin = PathBuf::from(program_files).join("Windows Kits/10/bin");
    let mut versions: Vec<PathBuf> = std::fs::read_dir(&bin)
        .map(|entries| entries.flatten().map(|e| e.path()).collect())
        .unwrap_or_default();
    versions.sort();
    for version in versions.iter().rev() {
        let candidate = version.join("x64/rc.exe");
        if candidate.is_file() {
            return candidate;
        }
    }
    panic!(
        "rc.exe (Windows SDK resource compiler) not found: run from a Visual Studio \
         developer shell, install the Windows SDK, or set RC to the path of rc.exe"
    );
}
