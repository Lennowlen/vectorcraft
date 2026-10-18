use android_activity::AndroidApp;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use vectorcraft_engine::Session;
use vectorcraft_ui_egui::{FilePick, Services, VectorcraftApp};
use jni::objects::JValue;

const WORKSPACE_DIR: &str = "/sdcard/Download/ArtCraft";

static ANDROID_APP: Mutex<Option<AndroidApp>> = Mutex::new(None);

fn return_to_launcher() {
    log::info!("return_to_launcher: Bringing Craft Launcher (ai.storyteller.craftlauncher) to foreground");
    let guard = ANDROID_APP.lock().unwrap();
    let Some(ref app) = *guard else { return };

    let vm_ptr = app.vm_as_ptr();
    let activity_ptr = app.activity_as_ptr();
    if vm_ptr.is_null() || activity_ptr.is_null() {
        return;
    }

    unsafe {
        let jvm = match jni::JavaVM::from_raw(vm_ptr as *mut jni::sys::JavaVM) {
            Ok(jvm) => jvm,
            Err(_) => return,
        };
        let mut env = match jvm.attach_current_thread() {
            Ok(env) => env,
            Err(_) => return,
        };
        let activity = match jni::objects::JObject::from_raw(activity_ptr as jni::sys::jobject) {
            activity if !activity.as_raw().is_null() => activity,
            _ => return,
        };

        let act_class = match env.find_class("android/content/Intent") {
            Ok(c) => c,
            Err(_) => return,
        };

        let pkg_jstring = match env.new_string("ai.storyteller.craftlauncher") {
            Ok(s) => s,
            Err(_) => return,
        };
        let cls_jstring = match env.new_string("android.app.NativeActivity") {
            Ok(s) => s,
            Err(_) => return,
        };

        let intent_obj = match env.new_object(&act_class, "()V", &[]) {
            Ok(obj) => obj,
            Err(_) => return,
        };

        let comp_class = match env.find_class("android/content/ComponentName") {
            Ok(c) => c,
            Err(_) => return,
        };

        let comp_obj = match env.new_object(&comp_class, "(Ljava/lang/String;Ljava/lang/String;)V", &[JValue::Object(&pkg_jstring), JValue::Object(&cls_jstring)]) {
            Ok(obj) => obj,
            Err(_) => return,
        };

        let _ = env.call_method(&intent_obj, "setComponent", "(Landroid/content/ComponentName;)Landroid/content/Intent;", &[JValue::Object(&comp_obj)]);

        let flags: i32 = 0x10000000 | 0x00020000;
        let _ = env.call_method(&intent_obj, "setFlags", "(I)Landroid/content/Intent;", &[JValue::Int(flags)]);
        let _ = env.call_method(&activity, "startActivity", "(Landroid/content/Intent;)V", &[JValue::Object(&intent_obj)]);
        if env.exception_check().unwrap_or(false) {
            let _ = env.exception_clear();
        }
    }
}

fn ensure_workspace_dir() -> PathBuf {
    let p = PathBuf::from(WORKSPACE_DIR);
    let _ = std::fs::create_dir_all(&p);
    p
}

fn create_android_services() -> Services {
    Services {
        pick_open: Some(Box::new(|_pick: &FilePick| {
            let base_dir = ensure_workspace_dir();
            let mut candidates = Vec::new();
            if let Ok(entries) = std::fs::read_dir(&base_dir) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("").to_ascii_lowercase();
                    if path.is_file() && (ext == "svg" || ext == "svgz" || ext == "png" || ext == "jpg" || ext == "jpeg" || ext == "pdf") {
                        candidates.push(path.to_string_lossy().into_owned());
                    }
                }
            }
            candidates.sort();
            candidates.first().cloned()
        })),
        pick_save: Some(Box::new(|pick: &FilePick| {
            let base_dir = ensure_workspace_dir();
            let name = if pick.name.is_empty() { "vector-artwork.svg" } else { &pick.name };
            let path = Path::new(name);
            let file_name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| name.to_string());
            let mut target = base_dir.join(file_name);
            if target.extension().is_none() {
                target.set_extension("svg");
            }
            Some(target.to_string_lossy().into_owned())
        })),
        read: Some(Box::new(|path: &str| std::fs::read(path).map_err(|e| e.to_string()))),
        write: Some(Box::new(|path: &str, bytes: &[u8]| std::fs::write(path, bytes).map_err(|e| e.to_string()))),
        write_shared: Some(std::sync::Arc::new(|path: &str, bytes: &[u8]| std::fs::write(path, bytes).map_err(|e| e.to_string()))),
        open_url: Some(Box::new(|url: &str| {
            if url == "app://launcher" {
                return_to_launcher();
            }
        })),
        ..Default::default()
    }
}

fn apply_immersive_sticky(app: &AndroidApp) {
    let vm_ptr = app.vm_as_ptr();
    let activity_ptr = app.activity_as_ptr();
    if vm_ptr.is_null() || activity_ptr.is_null() {
        return;
    }

    unsafe {
        let jvm = match jni::JavaVM::from_raw(vm_ptr as *mut jni::sys::JavaVM) {
            Ok(jvm) => jvm,
            Err(_) => return,
        };
        let mut env = match jvm.attach_current_thread() {
            Ok(env) => env,
            Err(_) => return,
        };
        let activity = match jni::objects::JObject::from_raw(activity_ptr as jni::sys::jobject) {
            activity if !activity.as_raw().is_null() => activity,
            _ => return,
        };

        if let Ok(window) = env.call_method(&activity, "getWindow", "()Landroid/view/Window;", &[]) {
            if let Ok(window_obj) = window.l() {
                if let Ok(decor_view) = env.call_method(&window_obj, "getDecorView", "()Landroid/view/View;", &[]) {
                    if let Ok(decor_obj) = decor_view.l() {
                        // SYSTEM_UI_FLAG_IMMERSIVE_STICKY (4096)
                        // | SYSTEM_UI_FLAG_FULLSCREEN (4)
                        // | SYSTEM_UI_FLAG_HIDE_NAVIGATION (2)
                        // | SYSTEM_UI_FLAG_LAYOUT_FULLSCREEN (1024)
                        // | SYSTEM_UI_FLAG_LAYOUT_HIDE_NAVIGATION (512)
                        // | SYSTEM_UI_FLAG_LAYOUT_STABLE (256)
                        let flags: i32 = 4096 | 4 | 2 | 1024 | 512 | 256;
                        let _ = env.call_method(&decor_obj, "setSystemUiVisibility", "(I)V", &[JValue::Int(flags)]);
                    }
                }
            }
        }
    }
}

struct VectorcraftAndroidApp {
    app: VectorcraftApp,
}

impl eframe::App for VectorcraftAndroidApp {
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.app.logic(ctx);
    }

    fn raw_input_hook(&mut self, _ctx: &egui::Context, raw: &mut egui::RawInput) {
        self.app.raw_input_hook(raw);
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.app.ui(ui);
    }
}

#[unsafe(no_mangle)]
fn android_main(app: AndroidApp) {
    android_logger::init_once(
        android_logger::Config::default()
            .with_max_level(log::LevelFilter::Info)
            .with_tag("VectorCraft"),
    );

    log::info!("Starting VectorCraft on Android ARM64...");

    *ANDROID_APP.lock().unwrap() = Some(app.clone());

    apply_immersive_sticky(&app);

    let mut options = eframe::NativeOptions::default();
    options.android_app = Some(app);

    let _ = eframe::run_native(
        "VectorCraft",
        options,
        Box::new(|_cc| {
            let session = Session::new();
            let services = create_android_services();
            let vector_app = VectorcraftApp::new(session, services);
            Ok(Box::new(VectorcraftAndroidApp { app: vector_app }))
        }),
    );
}
