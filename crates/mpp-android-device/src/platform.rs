//! Version-specific framework access. Every thread owns its JNI environment and local frames.

use std::{collections::BTreeMap, ffi::c_void, fmt};

use jni::{
    Env, JValue, JavaVM, jni_sig, jni_str,
    objects::{Global, JObject},
};
use mpp_core::{
    Error, InputEvent, KeyPhase, TouchPhase,
    stream::{AndroidFramework, Geometry, android_framework},
};

#[derive(Debug)]
pub enum NativeError {
    Core(Error),
    Jni(jni::errors::Error),
}
impl fmt::Display for NativeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Core(error) => error.fmt(f),
            Self::Jni(error) => write!(f, "Android JNI: {error}"),
        }
    }
}
impl std::error::Error for NativeError {}
impl From<Error> for NativeError {
    fn from(error: Error) -> Self {
        Self::Core(error)
    }
}
impl From<jni::errors::Error> for NativeError {
    fn from(error: jni::errors::Error) -> Self {
        Self::Jni(error)
    }
}
impl From<std::io::Error> for NativeError {
    fn from(error: std::io::Error) -> Self {
        Self::Core(Error::Io(error))
    }
}
pub type Result<T> = std::result::Result<T, NativeError>;
type Object = Global<JObject<'static>>;

pub fn failure(message: impl Into<String>) -> NativeError {
    Error::CommandFailed {
        tool: "MPP Android device".into(),
        message: message.into(),
    }
    .into()
}

pub fn check_platform(env: &mut Env<'_>) -> Result<()> {
    supported_framework(env).map(|_| ())
}

fn supported_framework(env: &mut Env<'_>) -> Result<AndroidFramework> {
    let sdk = env
        .get_static_field(
            jni_str!("android/os/Build$VERSION"),
            jni_str!("SDK_INT"),
            jni_sig!("I"),
        )?
        .i()?;
    let framework = u32::try_from(sdk).ok().and_then(android_framework);
    framework
        .filter(|_| cfg!(target_arch = "aarch64"))
        .ok_or_else(|| {
            Error::Unsupported {
                feature: format!(
                    "native preview requires Android 10–17 (API 29–37) on arm64; found API {sdk}"
                ),
            }
            .into()
        })
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DisplayInfo {
    pub width: u32,
    pub height: u32,
    pub rotation: u32,
    pub layer_stack: i32,
}

impl DisplayInfo {
    pub fn geometry(self, max_size: u32) -> Result<Geometry> {
        let major = self.width.max(self.height);
        if major == 0 || major > 16_384 || self.rotation > 3 {
            return Err(failure("invalid main display geometry"));
        }
        let scale = max_size.min(major);
        let align = |dimension: u32| {
            ((u64::from(dimension) * u64::from(scale) / u64::from(major)) as u32 / 16 * 16).max(16)
        };
        let geometry = Geometry {
            width: align(self.width),
            height: align(self.height),
            display_width: self.width,
            display_height: self.height,
            rotation: self.rotation,
        };
        geometry.validate()?;
        Ok(geometry)
    }
}

pub fn display_info(env: &mut Env<'_>) -> Result<DisplayInfo> {
    env.with_local_frame(16, |env| {
        let manager = env
            .call_static_method(
                jni_str!("android/hardware/display/DisplayManagerGlobal"),
                jni_str!("getInstance"),
                jni_sig!("()Landroid/hardware/display/DisplayManagerGlobal;"),
                &[],
            )?
            .l()?;
        let info = env
            .call_method(
                &manager,
                jni_str!("getDisplayInfo"),
                jni_sig!("(I)Landroid/view/DisplayInfo;"),
                &[JValue::Int(0)],
            )?
            .l()?;
        if info.is_null() {
            return Err(failure("main display is unavailable"));
        }
        let width = env
            .get_field(&info, jni_str!("logicalWidth"), jni_sig!("I"))?
            .i()?;
        let height = env
            .get_field(&info, jni_str!("logicalHeight"), jni_sig!("I"))?
            .i()?;
        let rotation = env
            .get_field(&info, jni_str!("rotation"), jni_sig!("I"))?
            .i()?;
        let layer_stack = env
            .get_field(&info, jni_str!("layerStack"), jni_sig!("I"))?
            .i()?;
        if width <= 0 || height <= 0 || !(0..=3).contains(&rotation) {
            return Err(failure("invalid main display properties"));
        }
        Ok(DisplayInfo {
            width: width as u32,
            height: height as u32,
            rotation: rotation as u32,
            layer_stack,
        })
    })
}

pub struct Capture {
    vm: JavaVM,
    display: Option<CaptureDisplay>,
    surface: Option<Object>,
}

enum CaptureDisplay {
    SurfaceControl(Object),
    VirtualDisplay(Object),
}

impl Capture {
    pub fn new(
        env: &mut Env<'_>,
        window: *mut c_void,
        display: DisplayInfo,
        geometry: Geometry,
    ) -> Result<Self> {
        let framework = supported_framework(env)?;
        let mut capture = Self {
            vm: env.get_java_vm()?,
            display: None,
            surface: None,
        };
        env.with_local_frame(24, |env| -> Result<()> {
            // SAFETY: window is the live, configured encoder's borrowed ANativeWindow. JNI acquires its own reference.
            let raw = unsafe { ANativeWindow_toSurface(env.get_raw(), window) };
            if raw.is_null() {
                return Err(failure("could not convert encoder input window to Surface"));
            }
            // SAFETY: ANativeWindow_toSurface returned a new JNI local reference in this frame.
            let surface = unsafe { JObject::from_raw(env, raw) };
            capture.surface = Some(env.new_global_ref(surface)?);
            let name = env.new_string("MPP preview")?;
            if framework == AndroidFramework::DisplayManager {
                // Android 14+ mirrors through DisplayManager rather than SurfaceControl tokens.
                // Capture display 0 into the encoder surface, without a new app/task display.
                let virtual_display = env
                    .call_static_method(
                        jni_str!("android/hardware/display/DisplayManager"),
                        jni_str!("createVirtualDisplay"),
                        jni_sig!(
                            "(Ljava/lang/String;IIILandroid/view/Surface;)Landroid/hardware/display/VirtualDisplay;"
                        ),
                        &[
                            JValue::Object(&name),
                            JValue::Int(geometry.width as i32),
                            JValue::Int(geometry.height as i32),
                            JValue::Int(0),
                            JValue::Object(capture.surface.as_ref().expect("created surface").as_obj()),
                        ],
                    )?
                    .l()?;
                if virtual_display.is_null() {
                    return Err(failure("DisplayManager returned no mirror display"));
                }
                let owned = match env.new_global_ref(&virtual_display) {
                    Ok(owned) => owned,
                    Err(error) => {
                        env.exception_clear();
                        let _ = env.call_method(
                            &virtual_display,
                            jni_str!("release"),
                            jni_sig!("()V"),
                            &[],
                        );
                        env.exception_clear();
                        return Err(error.into());
                    }
                };
                capture.display = Some(CaptureDisplay::VirtualDisplay(owned));
                return Ok(());
            }
            let token = env
                .call_static_method(
                    jni_str!("android/view/SurfaceControl"),
                    jni_str!("createDisplay"),
                    jni_sig!("(Ljava/lang/String;Z)Landroid/os/IBinder;"),
                    &[JValue::Object(&name), JValue::Bool(false)],
                )?
                .l()?;
            if token.is_null() {
                return Err(failure("SurfaceControl returned no display token"));
            }
            capture.display = Some(CaptureDisplay::SurfaceControl(env.new_global_ref(token)?));
            let source = env.new_object(
                jni_str!("android/graphics/Rect"),
                jni_sig!("(IIII)V"),
                &[
                    JValue::Int(0),
                    JValue::Int(0),
                    JValue::Int(display.width as i32),
                    JValue::Int(display.height as i32),
                ],
            )?;
            let target = env.new_object(
                jni_str!("android/graphics/Rect"),
                jni_sig!("(IIII)V"),
                &[
                    JValue::Int(0),
                    JValue::Int(0),
                    JValue::Int(geometry.width as i32),
                    JValue::Int(geometry.height as i32),
                ],
            )?;
            env.call_static_method(
                jni_str!("android/view/SurfaceControl"),
                jni_str!("openTransaction"),
                jni_sig!("()V"),
                &[],
            )?;
            let configured = (|| -> Result<()> {
                let Some(CaptureDisplay::SurfaceControl(token)) = capture.display.as_ref() else {
                    return Err(failure("missing SurfaceControl display"));
                };
                let token = token.as_obj();
                let surface = capture.surface.as_ref().expect("created surface").as_obj();
                env.call_static_method(
                    jni_str!("android/view/SurfaceControl"),
                    jni_str!("setDisplaySurface"),
                    jni_sig!("(Landroid/os/IBinder;Landroid/view/Surface;)V"),
                    &[JValue::Object(token), JValue::Object(surface)],
                )?;
                env.call_static_method(
                    jni_str!("android/view/SurfaceControl"),
                    jni_str!("setDisplayProjection"),
                    jni_sig!(
                        "(Landroid/os/IBinder;ILandroid/graphics/Rect;Landroid/graphics/Rect;)V"
                    ),
                    &[
                        JValue::Object(token),
                        JValue::Int(0),
                        JValue::Object(&source),
                        JValue::Object(&target),
                    ],
                )?;
                env.call_static_method(
                    jni_str!("android/view/SurfaceControl"),
                    jni_str!("setDisplayLayerStack"),
                    jni_sig!("(Landroid/os/IBinder;I)V"),
                    &[JValue::Object(token), JValue::Int(display.layer_stack)],
                )?;
                Ok(())
            })();
            if configured.is_err() {
                env.exception_clear();
            }
            let closed = env.call_static_method(
                jni_str!("android/view/SurfaceControl"),
                jni_str!("closeTransaction"),
                jni_sig!("()V"),
                &[],
            );
            configured?;
            closed?;
            Ok(())
        })?;
        Ok(capture)
    }
}

impl Drop for Capture {
    fn drop(&mut self) {
        let display = self.display.take();
        let surface = self.surface.take();
        let _ = self
            .vm
            .attach_current_thread(|env| -> jni::errors::Result<()> {
                // Cleanup must remain possible after a failed framework call left an exception pending.
                env.exception_clear();
                if let Some(display) = display {
                    match display {
                        CaptureDisplay::SurfaceControl(token) => {
                            let _ = env.call_static_method(
                                jni_str!("android/view/SurfaceControl"),
                                jni_str!("destroyDisplay"),
                                jni_sig!("(Landroid/os/IBinder;)V"),
                                &[JValue::Object(token.as_obj())],
                            );
                        }
                        CaptureDisplay::VirtualDisplay(display) => {
                            let _ = env.call_method(
                                display.as_obj(),
                                jni_str!("release"),
                                jni_sig!("()V"),
                                &[],
                            );
                        }
                    }
                    env.exception_clear();
                }
                if let Some(surface) = surface {
                    let _ = env.call_method(
                        surface.as_obj(),
                        jni_str!("release"),
                        jni_sig!("()V"),
                        &[],
                    );
                    env.exception_clear();
                }
                Ok(())
            });
    }
}

pub struct Injector {
    manager: Object,
    geometry: Geometry,
    touch: Option<(i64, f64, f64)>,
    keys: BTreeMap<u32, i64>,
}

impl Injector {
    pub fn new(env: &mut Env<'_>, geometry: Geometry) -> Result<Self> {
        let framework = supported_framework(env)?;
        let manager = env.with_local_frame(8, |env| -> Result<Object> {
            let manager = match framework {
                AndroidFramework::DisplayManager => env.call_static_method(
                    jni_str!("android/hardware/input/InputManagerGlobal"),
                    jni_str!("getInstance"),
                    jni_sig!("()Landroid/hardware/input/InputManagerGlobal;"),
                    &[],
                )?,
                AndroidFramework::SurfaceControl => env.call_static_method(
                    jni_str!("android/hardware/input/InputManager"),
                    jni_str!("getInstance"),
                    jni_sig!("()Landroid/hardware/input/InputManager;"),
                    &[],
                )?,
            }
            .l()?;
            if manager.is_null() {
                return Err(failure("Android InputManager is unavailable"));
            }
            Ok(env.new_global_ref(manager)?)
        })?;
        Ok(Self {
            manager,
            geometry,
            touch: None,
            keys: BTreeMap::new(),
        })
    }

    pub fn pressed(&self) -> bool {
        self.touch.is_some() || !self.keys.is_empty()
    }

    pub fn inject(&mut self, env: &mut Env<'_>, event: &InputEvent) -> Result<()> {
        let result = env.with_local_frame(16, |env| -> Result<()> {
            let now = env
                .call_static_method(
                    jni_str!("android/os/SystemClock"),
                    jni_str!("uptimeMillis"),
                    jni_sig!("()J"),
                    &[],
                )?
                .j()?;
            let native = match *event {
                InputEvent::Touch { phase, x, y, .. } => {
                    let down = self.touch.map_or(now, |touch| touch.0);
                    // A failed submission is uncertain; retain possible OS state until release succeeds.
                    if phase == TouchPhase::Down || self.touch.is_some() {
                        self.touch = Some((down, x, y));
                    }
                    let action = match phase {
                        TouchPhase::Down => 0,
                        TouchPhase::Up => 1,
                        TouchPhase::Move => 2,
                        TouchPhase::Cancel => 3,
                    };
                    let x = (x * f64::from(self.geometry.display_width - 1)) as f32;
                    let y = (y * f64::from(self.geometry.display_height - 1)) as f32;
                    let event = env
                        .call_static_method(
                            jni_str!("android/view/MotionEvent"),
                            jni_str!("obtain"),
                            jni_sig!("(JJIFFI)Landroid/view/MotionEvent;"),
                            &[
                                JValue::Long(down),
                                JValue::Long(now),
                                JValue::Int(action),
                                JValue::Float(x),
                                JValue::Float(y),
                                JValue::Int(0),
                            ],
                        )?
                        .l()?;
                    env.call_method(
                        &event,
                        jni_str!("setSource"),
                        jni_sig!("(I)V"),
                        &[JValue::Int(0x1002)],
                    )?;
                    event
                }
                InputEvent::Key { code, phase } => {
                    let down = *self.keys.entry(code).or_insert(now);
                    env.new_object(
                        jni_str!("android/view/KeyEvent"),
                        jni_sig!("(JJIIIIIIII)V"),
                        &[
                            JValue::Long(down),
                            JValue::Long(now),
                            JValue::Int(if phase == KeyPhase::Down { 0 } else { 1 }),
                            JValue::Int(code as i32),
                            JValue::Int(0),
                            JValue::Int(0),
                            JValue::Int(-1),
                            JValue::Int(0),
                            JValue::Int(0),
                            JValue::Int(0x101),
                        ],
                    )?
                }
                InputEvent::Text { .. } => {
                    return Err(Error::Unsupported {
                        feature: "device text injection".into(),
                    }
                    .into());
                }
            };
            let injection = (|| -> Result<()> {
                env.call_method(
                    &native,
                    jni_str!("setDisplayId"),
                    jni_sig!("(I)V"),
                    &[JValue::Int(0)],
                )?;
                let accepted = env
                    .call_method(
                        self.manager.as_obj(),
                        jni_str!("injectInputEvent"),
                        jni_sig!("(Landroid/view/InputEvent;I)Z"),
                        &[JValue::Object(&native), JValue::Int(0)],
                    )?
                    .z()?;
                if !accepted {
                    return Err(failure("Android rejected input submission"));
                }
                Ok(())
            })();
            if injection.is_err() {
                env.exception_clear();
            }
            if matches!(event, InputEvent::Touch { .. }) {
                let _ = env.call_method(&native, jni_str!("recycle"), jni_sig!("()V"), &[]);
                env.exception_clear();
            }
            injection?;
            match event {
                InputEvent::Touch {
                    phase: TouchPhase::Up | TouchPhase::Cancel,
                    ..
                } => self.touch = None,
                InputEvent::Key {
                    code,
                    phase: KeyPhase::Up,
                } => {
                    self.keys.remove(code);
                }
                _ => {}
            }
            Ok(())
        });
        if result.is_err() {
            env.exception_clear();
        }
        result
    }

    pub fn release_all(&mut self, env: &mut Env<'_>) {
        if let Some((_, x, y)) = self.touch {
            let _ = self.inject(
                env,
                &InputEvent::Touch {
                    phase: TouchPhase::Cancel,
                    x,
                    y,
                    width: self.geometry.width,
                    height: self.geometry.height,
                },
            );
        }
        let codes: Vec<_> = self.keys.keys().copied().collect();
        for code in codes {
            let _ = self.inject(
                env,
                &InputEvent::Key {
                    code,
                    phase: KeyPhase::Up,
                },
            );
        }
    }
}

#[link(name = "android")]
unsafe extern "C" {
    fn ANativeWindow_toSurface(
        env: *mut jni::sys::JNIEnv,
        window: *mut c_void,
    ) -> jni::sys::jobject;
}
