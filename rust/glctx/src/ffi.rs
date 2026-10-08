//! Southstar — the C ABI of the offscreen GL context, as declared in src/glctx.h, over WGL, CGL or EGL through libepoxy.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use southstar_glib::{FALSE, GBoolean, TRUE};

pub struct GlContext(backend::Context);

fn boolean(value: bool) -> GBoolean {
    if value { TRUE } else { FALSE }
}

#[unsafe(no_mangle)]
pub extern "C" fn ns_gl_context_create() -> *mut GlContext {
    backend::create().map_or(core::ptr::null_mut(), |context| {
        Box::into_raw(Box::new(GlContext(context)))
    })
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_gl_context_make_current(c: *mut GlContext) -> GBoolean {
    match unsafe { c.as_ref() } {
        Some(context) => boolean(backend::make_current(&context.0)),
        None => FALSE,
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_gl_context_release(c: *mut GlContext) {
    if let Some(context) = unsafe { c.as_ref() } {
        backend::release(&context.0);
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_gl_context_destroy(c: *mut GlContext) {
    if !c.is_null() {
        backend::destroy(unsafe { Box::from_raw(c) }.0);
    }
}

#[cfg(windows)]
mod backend {
    use core::ffi::c_void;
    use core::ptr;
    use std::sync::OnceLock;

    type Handle = *mut c_void;
    type WndProc = unsafe extern "system" fn(Handle, u32, usize, isize) -> isize;

    #[repr(C)]
    struct WndClassW {
        style: u32,
        wnd_proc: Option<WndProc>,
        cls_extra: i32,
        wnd_extra: i32,
        instance: Handle,
        icon: Handle,
        cursor: Handle,
        background: Handle,
        menu_name: *const u16,
        class_name: *const u16,
    }

    #[repr(C)]
    #[derive(Default)]
    struct PixelFormatDescriptor {
        size: u16,
        version: u16,
        flags: u32,
        pixel_type: u8,
        color_bits: u8,
        red_bits: u8,
        red_shift: u8,
        green_bits: u8,
        green_shift: u8,
        blue_bits: u8,
        blue_shift: u8,
        alpha_bits: u8,
        alpha_shift: u8,
        accum_bits: u8,
        accum_red_bits: u8,
        accum_green_bits: u8,
        accum_blue_bits: u8,
        accum_alpha_bits: u8,
        depth_bits: u8,
        stencil_bits: u8,
        aux_buffers: u8,
        layer_type: u8,
        reserved: u8,
        layer_mask: u32,
        visible_mask: u32,
        damage_mask: u32,
    }

    const CS_OWNDC: u32 = 0x0020;
    const WS_POPUP: u32 = 0x8000_0000;
    const PFD_DRAW_TO_WINDOW: u32 = 0x0000_0004;
    const PFD_SUPPORT_OPENGL: u32 = 0x0000_0020;
    const PFD_TYPE_RGBA: u8 = 0;

    #[link(name = "user32")]
    unsafe extern "system" {
        fn RegisterClassW(class: *const WndClassW) -> u16;
        fn DefWindowProcW(window: Handle, message: u32, wparam: usize, lparam: isize) -> isize;
        fn CreateWindowExW(
            ex_style: u32,
            class_name: *const u16,
            window_name: *const u16,
            style: u32,
            x: i32,
            y: i32,
            width: i32,
            height: i32,
            parent: Handle,
            menu: Handle,
            instance: Handle,
            param: *mut c_void,
        ) -> Handle;
        fn DestroyWindow(window: Handle) -> i32;
        fn GetDC(window: Handle) -> Handle;
        fn ReleaseDC(window: Handle, dc: Handle) -> i32;
    }

    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GetModuleHandleW(name: *const u16) -> Handle;
    }

    #[link(name = "gdi32")]
    unsafe extern "system" {
        fn ChoosePixelFormat(dc: Handle, descriptor: *const PixelFormatDescriptor) -> i32;
        fn SetPixelFormat(dc: Handle, format: i32, descriptor: *const PixelFormatDescriptor)
        -> i32;
    }

    #[link(name = "epoxy", kind = "dylib")]
    unsafe extern "C" {
        static epoxy_wglCreateContext: unsafe extern "system" fn(dc: Handle) -> Handle;
        static epoxy_wglDeleteContext: unsafe extern "system" fn(rc: Handle) -> i32;
        static epoxy_wglMakeCurrent: unsafe extern "system" fn(dc: Handle, rc: Handle) -> i32;
        static epoxy_wglGetCurrentContext: unsafe extern "system" fn() -> Handle;
        static epoxy_wglGetCurrentDC: unsafe extern "system" fn() -> Handle;
    }

    pub struct Context {
        window: Handle,
        dc: Handle,
        rc: Handle,
    }

    fn wide(text: &str) -> Vec<u16> {
        text.encode_utf16().chain([0]).collect()
    }

    fn class_name() -> *const u16 {
        static NAME: OnceLock<Vec<u16>> = OnceLock::new();
        NAME.get_or_init(|| {
            let name = wide("SouthstarGL");
            let class = WndClassW {
                style: CS_OWNDC,
                wnd_proc: Some(DefWindowProcW),
                cls_extra: 0,
                wnd_extra: 0,
                instance: unsafe { GetModuleHandleW(ptr::null()) },
                icon: ptr::null_mut(),
                cursor: ptr::null_mut(),
                background: ptr::null_mut(),
                menu_name: ptr::null(),
                class_name: name.as_ptr(),
            };
            unsafe { RegisterClassW(&class) };
            name
        })
        .as_ptr()
    }

    fn set_pixel_format(dc: Handle) -> bool {
        let descriptor = PixelFormatDescriptor {
            size: core::mem::size_of::<PixelFormatDescriptor>() as u16,
            version: 1,
            flags: PFD_SUPPORT_OPENGL | PFD_DRAW_TO_WINDOW,
            pixel_type: PFD_TYPE_RGBA,
            color_bits: 32,
            alpha_bits: 8,
            depth_bits: 24,
            stencil_bits: 8,
            ..PixelFormatDescriptor::default()
        };
        let format = unsafe { ChoosePixelFormat(dc, &descriptor) };
        format != 0 && unsafe { SetPixelFormat(dc, format, &descriptor) } != 0
    }

    pub fn create() -> Option<Context> {
        let title = [0u16];
        let window = unsafe {
            CreateWindowExW(
                0,
                class_name(),
                title.as_ptr(),
                WS_POPUP,
                0,
                0,
                1,
                1,
                ptr::null_mut(),
                ptr::null_mut(),
                GetModuleHandleW(ptr::null()),
                ptr::null_mut(),
            )
        };
        if window.is_null() {
            return None;
        }
        let dc = unsafe { GetDC(window) };
        if dc.is_null() || !set_pixel_format(dc) {
            unsafe {
                if !dc.is_null() {
                    ReleaseDC(window, dc);
                }
                DestroyWindow(window);
            }
            return None;
        }
        let rc = unsafe { epoxy_wglCreateContext(dc) };
        if rc.is_null() || unsafe { epoxy_wglMakeCurrent(dc, rc) } == 0 {
            unsafe {
                if !rc.is_null() {
                    epoxy_wglDeleteContext(rc);
                }
                ReleaseDC(window, dc);
                DestroyWindow(window);
            }
            return None;
        }
        Some(Context { window, dc, rc })
    }

    pub fn make_current(c: &Context) -> bool {
        unsafe {
            if epoxy_wglGetCurrentContext() == c.rc && epoxy_wglGetCurrentDC() == c.dc {
                return true;
            }
            epoxy_wglMakeCurrent(c.dc, c.rc) != 0
        }
    }

    pub fn release(c: &Context) {
        unsafe {
            if epoxy_wglGetCurrentContext() == c.rc {
                epoxy_wglMakeCurrent(ptr::null_mut(), ptr::null_mut());
            }
        }
    }

    pub fn destroy(c: Context) {
        release(&c);
        unsafe {
            epoxy_wglDeleteContext(c.rc);
            ReleaseDC(c.window, c.dc);
            DestroyWindow(c.window);
        }
    }
}

#[cfg(target_os = "macos")]
mod backend {
    use core::ffi::c_void;
    use core::ptr;

    type CglContext = *mut c_void;
    type CglPixelFormat = *mut c_void;
    const NO_ERROR: i32 = 0;
    const PFA_OPENGL_PROFILE: i32 = 99;
    const OGLP_VERSION_GL4_CORE: i32 = 0x4100;
    const PFA_COLOR_SIZE: i32 = 8;
    const PFA_ALPHA_SIZE: i32 = 11;
    const PFA_DEPTH_SIZE: i32 = 12;
    const PFA_STENCIL_SIZE: i32 = 13;

    unsafe extern "C" {
        fn CGLChoosePixelFormat(
            attributes: *const i32,
            pixel_format: *mut CglPixelFormat,
            count: *mut i32,
        ) -> i32;
        fn CGLDestroyPixelFormat(pixel_format: CglPixelFormat) -> i32;
        fn CGLCreateContext(
            pixel_format: CglPixelFormat,
            share: CglContext,
            context: *mut CglContext,
        ) -> i32;
        fn CGLDestroyContext(context: CglContext) -> i32;
        fn CGLSetCurrentContext(context: CglContext) -> i32;
        fn CGLGetCurrentContext() -> CglContext;
    }

    unsafe extern "C" {
        static epoxy_glGenVertexArrays: unsafe extern "C" fn(n: i32, arrays: *mut u32);
        static epoxy_glBindVertexArray: unsafe extern "C" fn(array: u32);
        static epoxy_glDeleteVertexArrays: unsafe extern "C" fn(n: i32, arrays: *const u32);
    }

    pub struct Context {
        ctx: CglContext,
        default_vao: u32,
    }

    pub fn create() -> Option<Context> {
        let attributes = [
            PFA_OPENGL_PROFILE,
            OGLP_VERSION_GL4_CORE,
            PFA_COLOR_SIZE,
            24,
            PFA_ALPHA_SIZE,
            8,
            PFA_DEPTH_SIZE,
            24,
            PFA_STENCIL_SIZE,
            8,
            0,
        ];
        let mut pixel_format = ptr::null_mut();
        let mut count = 0;
        if unsafe { CGLChoosePixelFormat(attributes.as_ptr(), &mut pixel_format, &mut count) }
            != NO_ERROR
            || pixel_format.is_null()
        {
            return None;
        }
        let mut ctx = ptr::null_mut();
        let err = unsafe { CGLCreateContext(pixel_format, ptr::null_mut(), &mut ctx) };
        unsafe { CGLDestroyPixelFormat(pixel_format) };
        if err != NO_ERROR || ctx.is_null() {
            return None;
        }
        if unsafe { CGLSetCurrentContext(ctx) } != NO_ERROR {
            unsafe { CGLDestroyContext(ctx) };
            return None;
        }
        let mut default_vao = 0;
        unsafe {
            epoxy_glGenVertexArrays(1, &mut default_vao);
            epoxy_glBindVertexArray(default_vao);
        }
        Some(Context { ctx, default_vao })
    }

    pub fn make_current(c: &Context) -> bool {
        unsafe { CGLGetCurrentContext() == c.ctx || CGLSetCurrentContext(c.ctx) == NO_ERROR }
    }

    pub fn release(c: &Context) {
        unsafe {
            if CGLGetCurrentContext() == c.ctx {
                CGLSetCurrentContext(ptr::null_mut());
            }
        }
    }

    pub fn destroy(c: Context) {
        unsafe {
            if CGLGetCurrentContext() == c.ctx {
                if c.default_vao != 0 {
                    epoxy_glDeleteVertexArrays(1, &c.default_vao);
                }
                CGLSetCurrentContext(ptr::null_mut());
            }
            CGLDestroyContext(c.ctx);
        }
    }
}

#[cfg(all(feature = "egl", not(windows), not(target_os = "macos")))]
mod backend {
    use crate::egl;
    use core::ffi::{CStr, c_char, c_void};
    use core::ptr;
    use std::sync::OnceLock;

    type Display = *mut c_void;
    type Config = *mut c_void;
    type EglContext = *mut c_void;
    type Surface = *mut c_void;
    type GetPlatformDisplay =
        unsafe extern "C" fn(platform: u32, native: *mut c_void, attributes: *const i32) -> Display;

    unsafe extern "C" {
        static epoxy_eglQueryString: unsafe extern "C" fn(Display, i32) -> *const c_char;
        static epoxy_eglGetProcAddress: unsafe extern "C" fn(*const c_char) -> *mut c_void;
        static epoxy_eglGetDisplay: unsafe extern "C" fn(*mut c_void) -> Display;
        static epoxy_eglInitialize: unsafe extern "C" fn(Display, *mut i32, *mut i32) -> u32;
        static epoxy_eglBindAPI: unsafe extern "C" fn(u32) -> u32;
        static epoxy_eglChooseConfig:
            unsafe extern "C" fn(Display, *const i32, *mut Config, i32, *mut i32) -> u32;
        static epoxy_eglCreateContext:
            unsafe extern "C" fn(Display, Config, EglContext, *const i32) -> EglContext;
        static epoxy_eglDestroyContext: unsafe extern "C" fn(Display, EglContext) -> u32;
        static epoxy_eglMakeCurrent:
            unsafe extern "C" fn(Display, Surface, Surface, EglContext) -> u32;
        static epoxy_eglGetCurrentContext: unsafe extern "C" fn() -> EglContext;
        static epoxy_eglCreatePbufferSurface:
            unsafe extern "C" fn(Display, Config, *const i32) -> Surface;
        static epoxy_eglDestroySurface: unsafe extern "C" fn(Display, Surface) -> u32;
    }

    pub struct Context {
        display: Display,
        context: EglContext,
        surface: Surface,
    }

    struct Shared(Display);

    unsafe impl Send for Shared {}
    unsafe impl Sync for Shared {}

    fn extensions(display: Display) -> &'static [u8] {
        let list = unsafe { epoxy_eglQueryString(display, egl::EXTENSIONS) };
        if list.is_null() {
            &[]
        } else {
            unsafe { CStr::from_ptr(list) }.to_bytes()
        }
    }

    fn surfaceless_display() -> Display {
        if !crate::has_extension(extensions(ptr::null_mut()), "EGL_MESA_platform_surfaceless") {
            return ptr::null_mut();
        }
        let address = unsafe { epoxy_eglGetProcAddress(c"eglGetPlatformDisplayEXT".as_ptr()) };
        if address.is_null() {
            return ptr::null_mut();
        }
        let get_platform_display: GetPlatformDisplay = unsafe { core::mem::transmute(address) };
        unsafe {
            get_platform_display(egl::PLATFORM_SURFACELESS_MESA, ptr::null_mut(), ptr::null())
        }
    }

    fn shared_display() -> Display {
        static SHARED: OnceLock<Shared> = OnceLock::new();
        SHARED
            .get_or_init(|| {
                let mut display = surfaceless_display();
                if display.is_null() {
                    display = unsafe { epoxy_eglGetDisplay(ptr::null_mut()) };
                }
                let (mut major, mut minor) = (0, 0);
                if !display.is_null()
                    && unsafe { epoxy_eglInitialize(display, &mut major, &mut minor) } != 0
                {
                    Shared(display)
                } else {
                    Shared(ptr::null_mut())
                }
            })
            .0
    }

    fn choose_config(display: Display) -> Option<Config> {
        let mut config = ptr::null_mut();
        let mut count = 0;
        for attributes in [&egl::PBUFFER_CONFIG[..], &egl::ANY_CONFIG[..]] {
            if unsafe {
                epoxy_eglChooseConfig(display, attributes.as_ptr(), &mut config, 1, &mut count)
            } != 0
                && count >= 1
            {
                return Some(config);
            }
        }
        None
    }

    pub fn create() -> Option<Context> {
        let display = shared_display();
        if display.is_null() || unsafe { epoxy_eglBindAPI(egl::OPENGL_ES_API) } == 0 {
            return None;
        }
        let config = choose_config(display)?;
        let robust = crate::has_extension(extensions(display), "EGL_EXT_create_context_robustness");
        let mut context = ptr::null_mut();
        for major in [3, 2] {
            if robust {
                let attributes = egl::robust_context(major);
                context = unsafe {
                    epoxy_eglCreateContext(display, config, ptr::null_mut(), attributes.as_ptr())
                };
            }
            if context.is_null() {
                let attributes = egl::plain_context(major);
                context = unsafe {
                    epoxy_eglCreateContext(display, config, ptr::null_mut(), attributes.as_ptr())
                };
            }
            if !context.is_null() {
                break;
            }
        }
        if context.is_null() {
            return None;
        }
        let mut surface = ptr::null_mut();
        if unsafe { epoxy_eglMakeCurrent(display, ptr::null_mut(), ptr::null_mut(), context) } == 0
        {
            surface = unsafe {
                epoxy_eglCreatePbufferSurface(display, config, egl::PBUFFER_SIZE.as_ptr())
            };
            if surface.is_null()
                || unsafe { epoxy_eglMakeCurrent(display, surface, surface, context) } == 0
            {
                unsafe {
                    if !surface.is_null() {
                        epoxy_eglDestroySurface(display, surface);
                    }
                    epoxy_eglDestroyContext(display, context);
                }
                return None;
            }
        }
        Some(Context {
            display,
            context,
            surface,
        })
    }

    pub fn make_current(c: &Context) -> bool {
        unsafe {
            epoxy_eglGetCurrentContext() == c.context
                || epoxy_eglMakeCurrent(c.display, c.surface, c.surface, c.context) != 0
        }
    }

    pub fn release(c: &Context) {
        unsafe {
            if epoxy_eglGetCurrentContext() == c.context {
                epoxy_eglMakeCurrent(c.display, ptr::null_mut(), ptr::null_mut(), ptr::null_mut());
            }
        }
    }

    pub fn destroy(c: Context) {
        unsafe {
            epoxy_eglMakeCurrent(c.display, ptr::null_mut(), ptr::null_mut(), ptr::null_mut());
            if !c.surface.is_null() {
                epoxy_eglDestroySurface(c.display, c.surface);
            }
            if !c.context.is_null() {
                epoxy_eglDestroyContext(c.display, c.context);
            }
        }
    }
}

#[cfg(not(any(windows, target_os = "macos", feature = "egl")))]
mod backend {
    pub enum Context {}

    pub fn create() -> Option<Context> {
        None
    }

    pub fn make_current(c: &Context) -> bool {
        match *c {}
    }

    pub fn release(c: &Context) {
        match *c {}
    }

    pub fn destroy(c: Context) {
        match c {}
    }
}
