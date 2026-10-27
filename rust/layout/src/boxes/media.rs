//! Southstar — the image, background, marker, border-image and video sources a box paints, and the borrowed handle layout and paint read them through.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::any::Any;
use core::cell::Cell;
use core::ffi::CStr;
use core::marker::PhantomData;
use std::rc::Rc;

use super::{peek, shared_cstr};

type Source = Option<Rc<CStr>>;

#[derive(Clone, Copy)]
pub struct MediaHandle {
    target: &'static dyn Any,
    stamp: u64,
}

impl MediaHandle {
    pub fn new(target: &'static dyn Any, stamp: u64) -> MediaHandle {
        MediaHandle { target, stamp }
    }

    pub fn target(self) -> &'static dyn Any {
        self.target
    }

    pub fn stamp(self) -> u64 {
        self.stamp
    }
}

type Handle = Option<MediaHandle>;

pub struct NsBoxMedia {
    image_src: Cell<Source>,
    image: Cell<Handle>,
    bg_image_src: Cell<Source>,
    bg_image: Cell<Handle>,
    marker_image_src: Cell<Source>,
    marker_image: Cell<Handle>,
    border_image_src: Cell<Source>,
    border_image: Cell<Handle>,
    bg_layer_srcs: Cell<Option<Rc<[Source]>>>,
    bg_layer_images: Cell<Option<Rc<[Handle]>>>,
    video_src: Cell<Source>,
    video_poster: Cell<Source>,
    video_audio_src: Cell<Source>,
    video: Cell<Handle>,
    declared_image_size: Cell<bool>,
    placeholder_image_size: Cell<bool>,
    size_independent_of_image: Cell<bool>,
    intrinsic_ratio_only: Cell<bool>,
    image_density: Cell<f64>,
}

impl NsBoxMedia {
    pub(super) fn vacant() -> NsBoxMedia {
        NsBoxMedia {
            image_src: Cell::new(None),
            image: Cell::new(None),
            bg_image_src: Cell::new(None),
            bg_image: Cell::new(None),
            marker_image_src: Cell::new(None),
            marker_image: Cell::new(None),
            border_image_src: Cell::new(None),
            border_image: Cell::new(None),
            bg_layer_srcs: Cell::new(None),
            bg_layer_images: Cell::new(None),
            video_src: Cell::new(None),
            video_poster: Cell::new(None),
            video_audio_src: Cell::new(None),
            video: Cell::new(None),
            declared_image_size: Cell::new(false),
            placeholder_image_size: Cell::new(false),
            size_independent_of_image: Cell::new(false),
            intrinsic_ratio_only: Cell::new(false),
            image_density: Cell::new(0.0),
        }
    }

    pub(super) fn reset(&self) {
        self.image_src.set(None);
        self.image.set(None);
        self.bg_image_src.set(None);
        self.bg_image.set(None);
        self.marker_image_src.set(None);
        self.marker_image.set(None);
        self.border_image_src.set(None);
        self.border_image.set(None);
        self.bg_layer_srcs.set(None);
        self.bg_layer_images.set(None);
        self.video_src.set(None);
        self.video_poster.set(None);
        self.video_audio_src.set(None);
        self.video.set(None);
        self.declared_image_size.set(false);
        self.placeholder_image_size.set(false);
        self.size_independent_of_image.set(false);
        self.intrinsic_ratio_only.set(false);
        self.image_density.set(0.0);
    }

    pub(super) fn bg_image_src_is_unset(&self) -> bool {
        let src = self.bg_image_src.take();
        let unset = src.is_none();
        self.bg_image_src.set(src);
        unset
    }
}

#[derive(Clone, Copy)]
pub struct MediaRef<'a>(&'static NsBoxMedia, PhantomData<&'a NsBoxMedia>);

impl<'a> MediaRef<'a> {
    pub(super) fn of(media: &'static NsBoxMedia) -> MediaRef<'a> {
        MediaRef(media, PhantomData)
    }

    pub fn image_src(self) -> Source {
        peek(&self.0.image_src)
    }

    pub fn bg_image_src(self) -> Source {
        peek(&self.0.bg_image_src)
    }

    pub fn marker_image_src(self) -> Source {
        peek(&self.0.marker_image_src)
    }

    pub fn border_image_src(self) -> Source {
        peek(&self.0.border_image_src)
    }

    pub fn bg_layer_srcs(self) -> Option<Rc<[Source]>> {
        peek(&self.0.bg_layer_srcs)
    }

    pub fn video_src(self) -> Source {
        peek(&self.0.video_src)
    }

    pub fn video_audio_src(self) -> Source {
        peek(&self.0.video_audio_src)
    }

    pub fn video_poster(self) -> Source {
        peek(&self.0.video_poster)
    }

    pub fn image(self) -> Handle {
        self.0.image.get()
    }

    pub fn bg_image(self) -> Handle {
        self.0.bg_image.get()
    }

    pub fn marker_image(self) -> Handle {
        self.0.marker_image.get()
    }

    pub fn border_image(self) -> Handle {
        self.0.border_image.get()
    }

    pub fn bg_layer_images(self) -> Option<Rc<[Handle]>> {
        peek(&self.0.bg_layer_images)
    }

    pub fn video(self) -> Handle {
        self.0.video.get()
    }

    pub fn set_video(self, video: Handle) {
        self.0.video.set(video);
    }

    pub fn declared_image_size(self) -> bool {
        self.0.declared_image_size.get()
    }

    pub fn placeholder_image_size(self) -> bool {
        self.0.placeholder_image_size.get()
    }

    pub fn intrinsic_ratio_only(self) -> bool {
        self.0.intrinsic_ratio_only.get()
    }

    pub fn image_density(self) -> f64 {
        self.0.image_density.get()
    }

    pub fn set_size_independent_of_image(self, independent: bool) {
        self.0.size_independent_of_image.set(independent);
    }

    pub fn set_image_src(self, src: Option<&CStr>) {
        self.0.image_src.set(shared_cstr(src));
    }

    pub fn set_image(self, image: Handle) {
        self.0.image.set(image);
    }

    pub fn set_image_density(self, density: f64) {
        self.0.image_density.set(density);
    }

    pub fn set_declared_image_size(self, declared: bool) {
        self.0.declared_image_size.set(declared);
    }

    pub fn set_placeholder_image_size(self, placeholder: bool) {
        self.0.placeholder_image_size.set(placeholder);
    }

    pub fn set_intrinsic_ratio_only(self, ratio_only: bool) {
        self.0.intrinsic_ratio_only.set(ratio_only);
    }

    pub fn set_video_src(self, src: Option<&CStr>) {
        self.0.video_src.set(shared_cstr(src));
    }

    pub fn set_video_poster(self, poster: Option<&CStr>) {
        self.0.video_poster.set(shared_cstr(poster));
    }

    pub fn set_video_audio_src(self, src: Option<&CStr>) {
        self.0.video_audio_src.set(shared_cstr(src));
    }

    pub fn set_marker_image(self, src: Option<&CStr>, image: Handle) {
        self.0.marker_image_src.set(shared_cstr(src));
        self.0.marker_image.set(image);
    }

    pub fn set_border_image(self, src: Option<&CStr>, image: Handle) {
        self.0.border_image_src.set(shared_cstr(src));
        self.0.border_image.set(image);
    }

    pub fn set_bg_image(self, src: Option<&CStr>, image: Handle) {
        self.0.bg_image_src.set(shared_cstr(src));
        self.0.bg_image.set(image);
    }

    pub fn set_bg_layers(self, srcs: Vec<Source>, images: Vec<Handle>) {
        self.0.bg_layer_srcs.set(Some(Rc::from(srcs)));
        self.0.bg_layer_images.set(Some(Rc::from(images)));
    }

    pub(super) fn set_bg_image_if_unset(self, src: &CStr, image: Handle) {
        if self.0.bg_image_src_is_unset() {
            self.set_bg_image(Some(src), image);
        }
    }
}
