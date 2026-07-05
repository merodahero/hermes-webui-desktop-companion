use serde::{Deserialize, Serialize};
use std::env;
use std::fs;
use std::net::{SocketAddr, TcpStream};
use std::path::{Path, PathBuf};
use std::process;
use std::process::{Child, Command, Stdio};
use std::sync::{atomic::AtomicBool, atomic::Ordering, Arc, Mutex};
use std::thread;
use std::time::Duration;
use tauri::menu::{MenuBuilder, SubmenuBuilder};
use tauri::path::BaseDirectory;
use tauri::{Emitter, Listener, Manager, Url, WebviewWindow};
use tauri_plugin_deep_link::DeepLinkExt;

const CLOSE_PET_MENU_ID: &str = "close_pet";
const MANAGE_PETS_MENU_ID: &str = "manage_pets";
const RESTART_PET_MENU_ID: &str = "restart_pet";
const COMPANION_DEEP_LINK_SCHEME: &str = "hermes-desktop-companion";
const LOOPBACK_SIDECAR_ADDR: &str = "127.0.0.1:17787";
const PET_NATIVE_RESTART_REQUESTED_EVENT: &str = "pet-native-restart-requested";
const PET_CONTEXT_MENU_EVENT: &str = "pet-context-menu";
const PET_SKIN_CHANGE_EVENT: &str = "pet-skin-change";
const PET_RESTART_REQUESTED_EVENT: &str = "pet-restart-requested";
const PET_RAISE_REQUESTED_EVENT: &str = "pet-raise-requested";
const PET_PERMISSION_TOGGLE_EVENT: &str = "pet-permission-toggle";
const SKIN_MENU_PREFIX: &str = "skin:";
const PERMISSION_MENU_PREFIX: &str = "permission:";

fn _persist_desktop_pet_preference(app: &tauri::AppHandle, enabled: bool) {
    let enabled_text = if enabled { "true" } else { "false" };
    let script = format!(
        "try{{const key='hermes-desktop-pet-enabled';const oldValue=localStorage.getItem(key);const token=window.__HERMES_CONFIG__&&window.__HERMES_CONFIG__.csrfToken;const headers={{'Content-Type':'application/json'}};if(token)headers['X-Hermes-CSRF-Token']=token;localStorage.setItem(key,'{enabled_text}');try{{window.dispatchEvent(new StorageEvent('storage',{{key,oldValue,newValue:'{enabled_text}',storageArea:localStorage,url:location.href}}));}}catch(_){{}}fetch('/api/pet/preference',{{method:'POST',credentials:'include',headers,body:JSON.stringify({{enabled:{enabled_text}}}),keepalive:true}}).catch(()=>{{}})}}catch(_){{}}"
    );
    for label in ["pet", "pet_bubbles"] {
        if let Some(window) = app.get_webview_window(label) {
            let _ = window.eval(&script);
        }
    }
}

fn _restart_native_process() {
    if let Ok(current_exe) = std::env::current_exe() {
        let _ = Command::new(current_exe).spawn();
    }
    thread::sleep(Duration::from_millis(80));
    process::exit(0);
}

fn lower_pet_windows_for_menu(app: &tauri::AppHandle) {
    for label in ["pet", "pet_bubbles"] {
        if let Some(window) = app.get_webview_window(label) {
            let _ = window.set_always_on_top(false);
        }
    }
}

#[cfg(target_os = "macos")]
fn set_native_window_level(window: &WebviewWindow, level: objc2_app_kit::NSWindowLevel) {
    use objc2_app_kit::NSWindow;
    if let Ok(ptr) = window.ns_window() {
        if !ptr.is_null() {
            let ns_window: &NSWindow = unsafe { &*ptr.cast() };
            ns_window.setLevel(level);
        }
    }
}

#[cfg(not(target_os = "macos"))]
fn set_native_window_level(_window: &WebviewWindow, _level: i32) {}

#[cfg(target_os = "macos")]
fn set_native_ignore_cursor_events(window: &WebviewWindow, ignore: bool) {
    use objc2_app_kit::NSWindow;
    if let Ok(ptr) = window.ns_window() {
        if !ptr.is_null() {
            let ns_window: &NSWindow = unsafe { &*ptr.cast() };
            ns_window.setIgnoresMouseEvents(ignore);
        }
    }
}

#[cfg(not(target_os = "macos"))]
fn set_native_ignore_cursor_events(_window: &WebviewWindow, _ignore: bool) {}

fn set_pet_window_level(window: &WebviewWindow) {
    #[cfg(target_os = "macos")]
    set_native_window_level(window, objc2_app_kit::NSStatusWindowLevel);
    #[cfg(not(target_os = "macos"))]
    set_native_window_level(window, 0);
}

fn set_bubble_window_level(window: &WebviewWindow) {
    #[cfg(target_os = "macos")]
    set_native_window_level(window, objc2_app_kit::NSStatusWindowLevel + 1);
    #[cfg(not(target_os = "macos"))]
    set_native_window_level(window, 0);
}

#[cfg(target_os = "macos")]
fn first_mouse_window_class() -> &'static objc2::runtime::AnyClass {
    use objc2::runtime::{AnyClass, AnyObject, Bool, ClassBuilder, Sel};
    use objc2::{msg_send, sel};

    extern "C-unwind" fn send_event(_this: &AnyObject, _cmd: Sel, event: &AnyObject) {
        let event_type: usize = unsafe { msg_send![event, type] };
        if matches!(event_type, 1 | 3) {
            let is_key: Bool = unsafe { msg_send![_this, isKeyWindow] };
            if !is_key.as_bool() {
                let _: () = unsafe { msg_send![_this, makeKeyWindow] };
            }
        }
        let superclass = objc2::class!(NSWindow);
        let _: () = unsafe { msg_send![super(_this, superclass), sendEvent: event] };
    }

    extern "C-unwind" fn can_become_key_window(_this: &AnyObject, _cmd: Sel) -> Bool {
        Bool::YES
    }

    extern "C-unwind" fn can_become_main_window(_this: &AnyObject, _cmd: Sel) -> Bool {
        Bool::YES
    }

    let class_name = c"HermesFirstClickWindow";
    if let Some(existing) = AnyClass::get(class_name) {
        return existing;
    }
    let mut builder = ClassBuilder::new(class_name, objc2::class!(NSWindow))
        .expect("failed to allocate first-click window class");
    unsafe {
        builder.add_method(
            sel!(sendEvent:),
            send_event as extern "C-unwind" fn(_, _, _),
        );
        builder.add_method(
            sel!(canBecomeKeyWindow),
            can_become_key_window as extern "C-unwind" fn(_, _) -> _,
        );
        builder.add_method(
            sel!(canBecomeMainWindow),
            can_become_main_window as extern "C-unwind" fn(_, _) -> _,
        );
    }
    builder.register()
}

#[cfg(target_os = "macos")]
fn install_first_click_handler(window: &WebviewWindow) {
    use objc2::runtime::AnyObject;

    if let Ok(ptr) = window.ns_window() {
        if !ptr.is_null() {
            let ns_window: &AnyObject = unsafe { &*ptr.cast() };
            let current_name = ns_window.class().name().to_string_lossy();
            if current_name.as_ref() != "HermesFirstClickWindow" {
                let next_class = first_mouse_window_class();
                unsafe {
                    let _ = AnyObject::set_class(ns_window, next_class);
                }
            }
        }
    }
}

#[cfg(not(target_os = "macos"))]
fn install_first_click_handler(_window: &WebviewWindow) {}

#[cfg(target_os = "macos")]
fn attach_bubble_child_window(pet_window: &WebviewWindow, bubble_window: &WebviewWindow) {
    use objc2_app_kit::{NSWindow, NSWindowOrderingMode};
    let Ok(pet_ptr) = pet_window.ns_window() else {
        return;
    };
    let Ok(bubble_ptr) = bubble_window.ns_window() else {
        return;
    };
    if pet_ptr.is_null() || bubble_ptr.is_null() {
        return;
    }
    let pet_ns_window: &NSWindow = unsafe { &*pet_ptr.cast() };
    let bubble_ns_window: &NSWindow = unsafe { &*bubble_ptr.cast() };
    unsafe {
        pet_ns_window.addChildWindow_ordered(bubble_ns_window, NSWindowOrderingMode::Above);
    }
}

#[cfg(not(target_os = "macos"))]
fn attach_bubble_child_window(_pet_window: &WebviewWindow, _bubble_window: &WebviewWindow) {}

fn restore_pet_window_layers(app: &tauri::AppHandle) {
    if let Some(pet_window) = app.get_webview_window("pet") {
        let _ = pet_window.set_ignore_cursor_events(false);
        set_native_ignore_cursor_events(&pet_window, false);
        let _ = pet_window.set_always_on_top(false);
        let _ = pet_window.set_always_on_top(true);
        set_pet_window_level(&pet_window);
        install_first_click_handler(&pet_window);
    }
    if let Some(bubble_window) = app.get_webview_window("pet_bubbles") {
        let _ = bubble_window.set_always_on_top(false);
        let _ = bubble_window.set_always_on_top(true);
        set_bubble_window_level(&bubble_window);
        install_first_click_handler(&bubble_window);
    }
}

fn restore_pet_window_layers_later(app: tauri::AppHandle, delay: Duration) {
    thread::spawn(move || {
        thread::sleep(delay);
        let handle_for_window = app.clone();
        let _ = app.run_on_main_thread(move || restore_pet_window_layers(&handle_for_window));
    });
}

fn restore_pet_window_layers_during_startup(app: tauri::AppHandle) {
    for delay in [
        Duration::from_millis(80),
        Duration::from_millis(300),
        Duration::from_millis(1200),
    ] {
        restore_pet_window_layers_later(app.clone(), delay);
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct PetRaisePayload {
    visible: Option<bool>,
    focus: Option<bool>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct PetAttentionUpdatePayload {
    count: Option<u64>,
    collapsed: Option<bool>,
}

fn parse_attention_visibility(payload: &str) -> bool {
    serde_json::from_str::<PetAttentionUpdatePayload>(payload)
        .map(|item| item.count.unwrap_or(0) > 0 && !item.collapsed.unwrap_or(false))
        .unwrap_or(false)
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct PetContextMenuPayload {
    skins: Vec<PetSkin>,
    active_skin_id: Option<String>,
    permissions: Option<PetPermissionsPayload>,
    menu_labels: Option<PetContextMenuLabels>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct PetContextMenuLabels {
    switch_skin: Option<String>,
    manage_pets: Option<String>,
    permissions_control: Option<String>,
    allow_direct_send: Option<String>,
    allow_inline_action_responses: Option<String>,
    restart_pet: Option<String>,
    close_pet: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
struct PetPermissionsPayload {
    allow_direct_send: Option<bool>,
    allow_inline_action_responses: Option<bool>,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct PetPermissionTogglePayload {
    key: String,
    enabled: bool,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct PetSkin {
    id: String,
    display_name: String,
}

fn desktop_pet_webui_base() -> String {
    let raw = std::env::var("HERMES_DESKTOP_PET_WEBUI_BASE")
        .or_else(|_| std::env::var("HERMES_DESKTOP_COMPANION_BASE"))
        .unwrap_or_else(|_| "http://127.0.0.1:17787".into());
    let trimmed = raw.trim().trim_end_matches('/');
    if let Ok(url) = Url::parse(trimmed) {
        let scheme_ok = matches!(url.scheme(), "http" | "https");
        let host_ok = matches!(
            url.host_str(),
            Some("127.0.0.1") | Some("localhost") | Some("::1")
        );
        if scheme_ok && host_ok {
            return trimmed.to_string();
        }
    }
    "http://127.0.0.1:17787".into()
}

fn navigate_window_to_webui(app: &tauri::App, label: &str, path: &str) {
    let Some(window) = app.get_webview_window(label) else {
        return;
    };
    let base = desktop_pet_webui_base();
    let Ok(mut url) = Url::parse(&format!("{}{}", base, path)) else {
        return;
    };
    url.query_pairs_mut()
        .append_pair("desktop_pet_pid", &process::id().to_string());
    let _ = window.navigate(url);
}

fn open_external_url(url: &str) {
    #[cfg(target_os = "macos")]
    {
        let _ = Command::new("open").arg(url).spawn();
    }
    #[cfg(target_os = "windows")]
    {
        let _ = Command::new("cmd").args(["/c", "start", "", url]).spawn();
    }
    #[cfg(all(not(target_os = "macos"), not(target_os = "windows")))]
    {
        let _ = Command::new("xdg-open").arg(url).spawn();
    }
}

fn open_pet_gallery_manager(_app: &tauri::AppHandle) {
    let base = desktop_pet_webui_base();
    open_external_url(&format!("{}/pet/gallery", base));
}

fn loopback_sidecar_running() -> bool {
    let Ok(addr) = LOOPBACK_SIDECAR_ADDR.parse::<SocketAddr>() else {
        return false;
    };
    TcpStream::connect_timeout(&addr, Duration::from_millis(180)).is_ok()
}

fn repo_root_from(start: &Path) -> Option<PathBuf> {
    let mut current = if start.is_file() {
        start.parent()?.to_path_buf()
    } else {
        start.to_path_buf()
    };
    loop {
        let package_json = current.join("package.json");
        let sidecar = current.join("src").join("loopback-server.mjs");
        if package_json.is_file()
            && sidecar.is_file()
            && fs::read_to_string(&package_json)
                .map(|text| text.contains("\"name\": \"hermes-webui-desktop-companion\""))
                .unwrap_or(false)
        {
            return Some(current);
        }
        if !current.pop() {
            return None;
        }
    }
}

fn env_path(name: &str) -> Option<PathBuf> {
    env::var_os(name)
        .map(PathBuf::from)
        .filter(|path| !path.as_os_str().is_empty())
}

fn repo_sidecar_target() -> Option<(PathBuf, PathBuf)> {
    if let Some(script) = env_path("HERMES_DESKTOP_COMPANION_SIDECAR_SCRIPT") {
        if script.is_file() {
            let root = script
                .parent()
                .and_then(Path::parent)
                .map(Path::to_path_buf)
                .unwrap_or_else(|| PathBuf::from("."));
            return Some((script, root));
        }
    }
    if let Some(root) =
        env_path("HERMES_DESKTOP_COMPANION_REPO").and_then(|path| repo_root_from(&path))
    {
        return Some((root.join("src").join("loopback-server.mjs"), root));
    }
    for candidate in [env::current_dir().ok(), env::current_exe().ok()]
        .into_iter()
        .flatten()
    {
        if let Some(root) = repo_root_from(&candidate) {
            return Some((root.join("src").join("loopback-server.mjs"), root));
        }
    }
    None
}

fn bundled_sidecar_target(app: &tauri::AppHandle) -> Option<(PathBuf, PathBuf)> {
    let script = app
        .path()
        .resolve("companion/src/loopback-server.mjs", BaseDirectory::Resource)
        .ok()?;
    if !script.is_file() {
        return None;
    }
    let root = script.parent()?.parent()?.to_path_buf();
    Some((script, root))
}

fn sidecar_launch_target(app: &tauri::AppHandle) -> Option<(PathBuf, PathBuf)> {
    bundled_sidecar_target(app).or_else(repo_sidecar_target)
}

fn node_candidates() -> Vec<PathBuf> {
    let mut candidates = Vec::new();
    if let Some(path) = env_path("HERMES_DESKTOP_COMPANION_NODE") {
        candidates.push(path);
    }
    candidates.extend([
        PathBuf::from("node"),
        PathBuf::from("/opt/homebrew/bin/node"),
        PathBuf::from("/usr/local/bin/node"),
    ]);
    candidates
}

fn sidecar_child_still_running(sidecar_process: &Arc<Mutex<Option<Child>>>) -> bool {
    let Ok(mut child_guard) = sidecar_process.lock() else {
        return false;
    };
    let Some(child) = child_guard.as_mut() else {
        return false;
    };
    match child.try_wait() {
        Ok(None) => true,
        Ok(Some(_)) | Err(_) => {
            *child_guard = None;
            false
        }
    }
}

fn wait_for_loopback_sidecar(timeout: Duration) -> bool {
    let deadline = std::time::Instant::now() + timeout;
    while std::time::Instant::now() < deadline {
        if loopback_sidecar_running() {
            return true;
        }
        thread::sleep(Duration::from_millis(80));
    }
    loopback_sidecar_running()
}

fn ensure_loopback_sidecar(app: &tauri::AppHandle, sidecar_process: &Arc<Mutex<Option<Child>>>) {
    if loopback_sidecar_running() || sidecar_child_still_running(sidecar_process) {
        return;
    }
    let Some((script, root)) = sidecar_launch_target(app) else {
        eprintln!("[desktop-companion] sidecar script was not found");
        return;
    };
    for node in node_candidates() {
        let spawn_result = Command::new(&node)
            .arg(&script)
            .current_dir(&root)
            .env(
                "HERMES_DESKTOP_COMPANION_BASE",
                format!("http://{LOOPBACK_SIDECAR_ADDR}"),
            )
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn();
        match spawn_result {
            Ok(child) => {
                if let Ok(mut child_guard) = sidecar_process.lock() {
                    *child_guard = Some(child);
                }
                let _ = wait_for_loopback_sidecar(Duration::from_secs(3));
                return;
            }
            Err(error) => {
                eprintln!(
                    "[desktop-companion] failed to start sidecar with {}: {error}",
                    node.display()
                );
            }
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum CompanionDeepLinkAction {
    Start,
    Gallery,
}

fn companion_deep_link_action(raw: &str) -> Option<CompanionDeepLinkAction> {
    let url = Url::parse(raw).ok()?;
    if url.scheme() != COMPANION_DEEP_LINK_SCHEME {
        return None;
    }
    let host = url.host_str().unwrap_or("").trim_matches('/');
    let path = url.path().trim_matches('/');
    let action = if !host.is_empty() {
        host
    } else if !path.is_empty() {
        path.split('/').next().unwrap_or("")
    } else {
        "start"
    };
    match action {
        "" | "start" => Some(CompanionDeepLinkAction::Start),
        "gallery" | "manager" | "pets" => Some(CompanionDeepLinkAction::Gallery),
        _ => None,
    }
}

fn handle_companion_deep_link(
    app: &tauri::AppHandle,
    sidecar_process: &Arc<Mutex<Option<Child>>>,
    raw: &str,
) {
    let Some(action) = companion_deep_link_action(raw) else {
        return;
    };
    ensure_loopback_sidecar(app, sidecar_process);
    restore_pet_window_layers(app);
    if let Some(window) = app.get_webview_window("pet") {
        let _ = window.show();
        let _ = window.set_focus();
    }
    if action == CompanionDeepLinkAction::Gallery {
        open_pet_gallery_manager(app);
    }
}

fn apply_bubble_visibility(
    app: &tauri::AppHandle,
    visible_state: &Arc<Mutex<bool>>,
    visible: bool,
    focus: bool,
) {
    if let Ok(mut state) = visible_state.lock() {
        *state = visible;
    }
    let Some(bubble_window) = app.get_webview_window("pet_bubbles") else {
        return;
    };
    let _ = bubble_window.set_ignore_cursor_events(!visible);
    set_native_ignore_cursor_events(&bubble_window, !visible);
    if visible {
        let _ = bubble_window.set_always_on_top(true);
        set_bubble_window_level(&bubble_window);
        install_first_click_handler(&bubble_window);
        let _ = bubble_window.show();
        if focus {
            let _ = bubble_window.set_focus();
        }
    } else {
        let _ = bubble_window.hide();
    }
}

fn fallback_skins() -> Vec<PetSkin> {
    vec![
        PetSkin {
            id: "keeper".into(),
            display_name: "May".into(),
        },
        PetSkin {
            id: "shiba".into(),
            display_name: "shiba".into(),
        },
    ]
}

fn pet_context_menu_payload(payload: &str) -> PetContextMenuPayload {
    serde_json::from_str(payload).unwrap_or_else(|_| PetContextMenuPayload {
        skins: fallback_skins(),
        active_skin_id: Some("keeper".into()),
        permissions: None,
        menu_labels: None,
    })
}

fn menu_label(value: Option<&String>, fallback: &str) -> String {
    let label = value
        .map(|raw| raw.trim())
        .filter(|raw| !raw.is_empty())
        .unwrap_or(fallback);
    let cleaned = label
        .chars()
        .filter(|ch| !ch.is_control())
        .take(64)
        .collect::<String>()
        .trim()
        .to_string();
    if cleaned.is_empty() {
        fallback.into()
    } else {
        cleaned
    }
}

fn valid_skin_id(id: &str) -> bool {
    !id.is_empty()
        && id
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || ch == '_' || ch == '-')
}

fn sanitize_skin(skin: PetSkin) -> Option<PetSkin> {
    if !valid_skin_id(&skin.id) {
        return None;
    }
    let display_name = skin
        .display_name
        .trim()
        .chars()
        .filter(|ch| !ch.is_control())
        .take(64)
        .collect::<String>();
    Some(PetSkin {
        id: skin.id,
        display_name: if display_name.is_empty() {
            "skin".into()
        } else {
            display_name
        },
    })
}

fn main() {
    let restart_requested = Arc::new(AtomicBool::new(false));
    let restart_requested_for_setup = restart_requested.clone();
    let restart_requested_for_menu = restart_requested.clone();
    let bubble_visible_state = Arc::new(Mutex::new(false));
    let bubble_visible_state_for_setup = bubble_visible_state.clone();
    let sidecar_process: Arc<Mutex<Option<Child>>> = Arc::new(Mutex::new(None));
    let sidecar_for_setup = sidecar_process.clone();
    let sidecar_for_single_instance = sidecar_process.clone();
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(
            move |app, argv, _cwd| {
                let handle = app.clone();
                for raw in argv {
                    if companion_deep_link_action(&raw).is_none() {
                        continue;
                    }
                    let deep_link_handle = handle.clone();
                    let deep_link_sidecar = sidecar_for_single_instance.clone();
                    let _ = handle.run_on_main_thread(move || {
                        handle_companion_deep_link(&deep_link_handle, &deep_link_sidecar, &raw);
                    });
                }
            },
        ))
        .plugin(tauri_plugin_deep_link::init())
        .setup(move |app| {
            ensure_loopback_sidecar(app.handle(), &sidecar_for_setup);
            navigate_window_to_webui(app, "pet", "/pet");
            navigate_window_to_webui(app, "pet_bubbles", "/pet/bubbles");
            if let Some(pet_window) = app.get_webview_window("pet") {
                let _ = pet_window.set_ignore_cursor_events(false);
                set_native_ignore_cursor_events(&pet_window, false);
                let _ = pet_window.set_always_on_top(true);
                set_pet_window_level(&pet_window);
                install_first_click_handler(&pet_window);
            }
            if let Some(bubble_window) = app.get_webview_window("pet_bubbles") {
                let _ = bubble_window.set_ignore_cursor_events(true);
                set_native_ignore_cursor_events(&bubble_window, true);
                let _ = bubble_window.set_always_on_top(true);
                set_bubble_window_level(&bubble_window);
                install_first_click_handler(&bubble_window);
            }
            if let (Some(pet_window), Some(bubble_window)) = (
                app.get_webview_window("pet"),
                app.get_webview_window("pet_bubbles"),
            ) {
                attach_bubble_child_window(&pet_window, &bubble_window);
            }
            restore_pet_window_layers_during_startup(app.handle().clone());
            if let Ok(Some(urls)) = app.deep_link().get_current() {
                for url in urls {
                    handle_companion_deep_link(app.handle(), &sidecar_for_setup, url.as_str());
                }
            }
            let deep_link_handle = app.handle().clone();
            let deep_link_sidecar = sidecar_for_setup.clone();
            app.deep_link().on_open_url(move |event| {
                let urls: Vec<String> = event.urls().iter().map(ToString::to_string).collect();
                for raw in urls {
                    let handle = deep_link_handle.clone();
                    let sidecar = deep_link_sidecar.clone();
                    let runner = handle.clone();
                    let _ = runner.run_on_main_thread(move || {
                        handle_companion_deep_link(&handle, &sidecar, &raw);
                    });
                }
            });
            let raise_handle = app.handle().clone();
            let raise_visible_state = bubble_visible_state_for_setup.clone();
            app.listen(PET_RAISE_REQUESTED_EVENT, move |event| {
                let handle = raise_handle.clone();
                let window_handle = handle.clone();
                let runner_handle = handle.clone();
                let control_handle = handle.clone();
                let visible_state = raise_visible_state.clone();
                let payload = serde_json::from_str::<PetRaisePayload>(event.payload()).ok();
                let visible = payload
                    .as_ref()
                    .and_then(|payload| payload.visible)
                    .unwrap_or(true);
                let focus = payload
                    .as_ref()
                    .and_then(|payload| payload.focus)
                    .unwrap_or(false);
                let _ = runner_handle.run_on_main_thread(move || {
                    apply_bubble_visibility(&control_handle, &visible_state, visible, focus);
                    if let Some(window) = window_handle.get_webview_window("pet") {
                        let _ = window.set_ignore_cursor_events(false);
                        set_native_ignore_cursor_events(&window, false);
                        set_pet_window_level(&window);
                        install_first_click_handler(&window);
                        let _ = window.show();
                    }
                });
            });
            let app_handle = app.handle().clone();
            let attention_visible_state = bubble_visible_state_for_setup.clone();
            app.listen("pet-attention-update", move |event| {
                let handle = app_handle.clone();
                let visible = parse_attention_visibility(event.payload());
                let handle_for_window = handle.clone();
                let visible_state = attention_visible_state.clone();
                let should_apply = visible_state
                    .lock()
                    .map(|state| *state != visible)
                    .unwrap_or(true);
                let _ = handle.run_on_main_thread(move || {
                    if should_apply {
                        apply_bubble_visibility(&handle_for_window, &visible_state, visible, false);
                    }
                });
            });
            let restart_requested = restart_requested_for_setup.clone();
            app.listen(PET_NATIVE_RESTART_REQUESTED_EVENT, move |_| {
                if restart_requested.swap(true, Ordering::SeqCst) {
                    return;
                }
                thread::spawn(_restart_native_process);
            });
            let handle = app.handle().clone();
            app.listen(PET_CONTEXT_MENU_EVENT, move |event| {
                let payload = pet_context_menu_payload(event.payload());
                let handle = handle.clone();
                let menu_handle = handle.clone();
                let _ = handle.run_on_main_thread(move || {
                    let Some(window) = menu_handle.get_webview_window("pet") else {
                        return;
                    };
                    lower_pet_windows_for_menu(&menu_handle);
                    let labels = payload.menu_labels.as_ref();
                    let switch_skin_label = menu_label(
                        labels.and_then(|item| item.switch_skin.as_ref()),
                        "Switch skin",
                    );
                    let manage_pets_label = menu_label(
                        labels.and_then(|item| item.manage_pets.as_ref()),
                        "Manage pets...",
                    );
                    let permissions_control_label = menu_label(
                        labels.and_then(|item| item.permissions_control.as_ref()),
                        "Permission control",
                    );
                    let allow_direct_send_label = menu_label(
                        labels.and_then(|item| item.allow_direct_send.as_ref()),
                        "Direct send",
                    );
                    let allow_inline_action_responses_label = menu_label(
                        labels.and_then(|item| item.allow_inline_action_responses.as_ref()),
                        "Approval / clarify responses",
                    );
                    let restart_pet_label = menu_label(
                        labels.and_then(|item| item.restart_pet.as_ref()),
                        "Restart pet",
                    );
                    let close_pet_label =
                        menu_label(labels.and_then(|item| item.close_pet.as_ref()), "Close pet");
                    let mut skin_builder = SubmenuBuilder::new(&menu_handle, switch_skin_label);
                    let active_skin_id = payload
                        .active_skin_id
                        .as_deref()
                        .filter(|id| valid_skin_id(id))
                        .unwrap_or("keeper");
                    let mut skins: Vec<PetSkin> = payload
                        .skins
                        .into_iter()
                        .filter_map(sanitize_skin)
                        .collect();
                    if skins.is_empty() {
                        skins = fallback_skins();
                    }
                    for skin in skins {
                        let mut label = skin.display_name;
                        if skin.id == active_skin_id {
                            label = format!("{} ✓", label);
                        }
                        skin_builder =
                            skin_builder.text(format!("{SKIN_MENU_PREFIX}{}", skin.id), label);
                    }
                    let Ok(skin_menu) = skin_builder.build() else {
                        return;
                    };
                    let allow_direct_send = payload
                        .permissions
                        .as_ref()
                        .and_then(|item| item.allow_direct_send)
                        .unwrap_or(false);
                    let allow_inline_action_responses = payload
                        .permissions
                        .as_ref()
                        .and_then(|item| item.allow_inline_action_responses)
                        .unwrap_or(false);
                    let direct_send_label = if allow_direct_send {
                        format!("{} ✓", allow_direct_send_label)
                    } else {
                        allow_direct_send_label
                    };
                    let inline_action_label = if allow_inline_action_responses {
                        format!("{} ✓", allow_inline_action_responses_label)
                    } else {
                        allow_inline_action_responses_label
                    };
                    let Ok(permission_menu) =
                        SubmenuBuilder::new(&menu_handle, permissions_control_label)
                            .text(
                                format!(
                                    "{PERMISSION_MENU_PREFIX}allow_direct_send:{}",
                                    !allow_direct_send
                                ),
                                direct_send_label,
                            )
                            .text(
                                format!(
                                    "{PERMISSION_MENU_PREFIX}allow_inline_action_responses:{}",
                                    !allow_inline_action_responses
                                ),
                                inline_action_label,
                            )
                            .build()
                    else {
                        return;
                    };
                    let Ok(menu) = MenuBuilder::new(&menu_handle)
                        .item(&skin_menu)
                        .text(MANAGE_PETS_MENU_ID, manage_pets_label)
                        .separator()
                        .item(&permission_menu)
                        .separator()
                        .text(RESTART_PET_MENU_ID, restart_pet_label)
                        .text(CLOSE_PET_MENU_ID, close_pet_label)
                        .build()
                    else {
                        return;
                    };
                    let _ = window.popup_menu(&menu);
                    restore_pet_window_layers_later(menu_handle.clone(), Duration::from_secs(12));
                });
            });
            Ok(())
        })
        .on_menu_event(move |app, event| {
            let id = event.id().as_ref();
            if let Some(skin_id) = id.strip_prefix(SKIN_MENU_PREFIX) {
                if !valid_skin_id(skin_id) {
                    return;
                }
                let skin_id = skin_id.to_string();
                let _ = app.emit_to("pet", PET_SKIN_CHANGE_EVENT, skin_id.clone());
                let _ = app.emit_to("pet_bubbles", PET_SKIN_CHANGE_EVENT, skin_id);
                restore_pet_window_layers(&app.clone());
                return;
            }
            if let Some(raw) = id.strip_prefix(PERMISSION_MENU_PREFIX) {
                let mut parts = raw.split(':');
                let Some(key) = parts.next() else {
                    return;
                };
                let Some(enabled_text) = parts.next() else {
                    return;
                };
                if !matches!(key, "allow_direct_send" | "allow_inline_action_responses") {
                    return;
                }
                let enabled = enabled_text == "true";
                let payload = PetPermissionTogglePayload {
                    key: key.to_string(),
                    enabled,
                };
                restore_pet_window_layers(&app.clone());
                let _ = app.emit_to("pet", PET_PERMISSION_TOGGLE_EVENT, payload.clone());
                let _ = app.emit_to("pet_bubbles", PET_PERMISSION_TOGGLE_EVENT, payload);
                return;
            }
            match id {
                CLOSE_PET_MENU_ID => {
                    _persist_desktop_pet_preference(&app.clone(), false);
                    let exit_handle = app.clone();
                    thread::spawn(move || {
                        thread::sleep(Duration::from_millis(220));
                        exit_handle.exit(0);
                    });
                }
                MANAGE_PETS_MENU_ID => {
                    restore_pet_window_layers(&app.clone());
                    open_pet_gallery_manager(&app.clone());
                }
                RESTART_PET_MENU_ID => {
                    restore_pet_window_layers(&app.clone());
                    let _ = app.emit_to("pet", PET_RESTART_REQUESTED_EVENT, ());
                    let _ = app.emit_to("pet_bubbles", PET_RESTART_REQUESTED_EVENT, ());
                    let app_for_fallback = app.clone();
                    let should_restart = restart_requested_for_menu.clone();
                    thread::spawn(move || {
                        thread::sleep(Duration::from_millis(260));
                        if should_restart.swap(true, Ordering::SeqCst) {
                            return;
                        }
                        let _ = app_for_fallback.emit_to("pet", PET_RESTART_REQUESTED_EVENT, ());
                        _restart_native_process();
                    });
                }
                _ => {
                    restore_pet_window_layers(&app.clone());
                }
            }
        })
        .run(tauri::generate_context!())
        .expect("failed to run Hermes desktop pet");
}
