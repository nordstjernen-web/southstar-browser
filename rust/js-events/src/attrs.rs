//! Southstar — the attributes of the event interfaces, as getters on their prototypes that read the event's engine state.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use southstar_js_engine::{Scope, Value};

use crate::{ENUMERABLE_CONFIGURABLE, JsResult, arg, ffi, get};

#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    Number,
    Bool,
    Str,
    Null,
    Undefined,
    Array,
}

struct Attr {
    iface: &'static str,
    name: &'static str,
    kind: Kind,
    writable: bool,
}

const fn attr(iface: &'static str, name: &'static str, kind: Kind, writable: bool) -> Attr {
    Attr {
        iface,
        name,
        kind,
        writable,
    }
}

static ATTRS: &[Attr] = &[
    attr("AnimationEvent", "animation", Kind::Null, false),
    attr("AnimationEvent", "animationName", Kind::Str, false),
    attr("AnimationEvent", "elapsedTime", Kind::Number, false),
    attr("AnimationEvent", "pseudoElement", Kind::Str, false),
    attr("AnimationEvent", "pseudoTarget", Kind::Null, false),
    attr("AnimationPlaybackEvent", "currentTime", Kind::Null, false),
    attr("AnimationPlaybackEvent", "timelineTime", Kind::Null, false),
    attr("AudioProcessingEvent", "inputBuffer", Kind::Null, false),
    attr("AudioProcessingEvent", "outputBuffer", Kind::Null, false),
    attr("AudioProcessingEvent", "playbackTime", Kind::Null, false),
    attr("BeforeInstallPromptEvent", "platforms", Kind::Array, false),
    attr("BeforeInstallPromptEvent", "userChoice", Kind::Null, false),
    attr("BeforeUnloadEvent", "returnValue", Kind::Null, true),
    attr("BlobEvent", "data", Kind::Null, false),
    attr("BlobEvent", "timecode", Kind::Number, false),
    attr(
        "CharacterBoundsUpdateEvent",
        "rangeEnd",
        Kind::Number,
        false,
    ),
    attr(
        "CharacterBoundsUpdateEvent",
        "rangeStart",
        Kind::Number,
        false,
    ),
    attr("ClipboardChangeEvent", "changeId", Kind::Null, false),
    attr("ClipboardChangeEvent", "types", Kind::Null, false),
    attr("ClipboardEvent", "clipboardData", Kind::Null, false),
    attr("CloseEvent", "code", Kind::Number, false),
    attr("CloseEvent", "reason", Kind::Str, false),
    attr("CloseEvent", "wasClean", Kind::Bool, false),
    attr("CommandEvent", "command", Kind::Str, false),
    attr("CommandEvent", "source", Kind::Null, false),
    attr("CompositionEvent", "data", Kind::Str, false),
    attr(
        "ContentVisibilityAutoStateChangeEvent",
        "skipped",
        Kind::Bool,
        false,
    ),
    attr("CookieChangeEvent", "changed", Kind::Array, false),
    attr("CookieChangeEvent", "deleted", Kind::Array, false),
    attr("CustomEvent", "detail", Kind::Null, false),
    attr("DeviceMotionEvent", "acceleration", Kind::Null, false),
    attr(
        "DeviceMotionEvent",
        "accelerationIncludingGravity",
        Kind::Null,
        false,
    ),
    attr("DeviceMotionEvent", "interval", Kind::Number, false),
    attr("DeviceMotionEvent", "rotationRate", Kind::Null, false),
    attr("DeviceOrientationEvent", "absolute", Kind::Bool, false),
    attr("DeviceOrientationEvent", "alpha", Kind::Null, false),
    attr("DeviceOrientationEvent", "beta", Kind::Null, false),
    attr("DeviceOrientationEvent", "gamma", Kind::Null, false),
    attr("DocumentPictureInPictureEvent", "window", Kind::Null, false),
    attr("DragEvent", "dataTransfer", Kind::Null, false),
    attr("ErrorEvent", "colno", Kind::Number, false),
    attr("ErrorEvent", "error", Kind::Undefined, false),
    attr("ErrorEvent", "filename", Kind::Str, false),
    attr("ErrorEvent", "lineno", Kind::Number, false),
    attr("ErrorEvent", "message", Kind::Str, false),
    attr("Event", "bubbles", Kind::Bool, false),
    attr("Event", "cancelBubble", Kind::Bool, true),
    attr("Event", "cancelable", Kind::Bool, false),
    attr("Event", "composed", Kind::Bool, false),
    attr("Event", "currentTarget", Kind::Null, false),
    attr("Event", "defaultPrevented", Kind::Bool, false),
    attr("Event", "eventPhase", Kind::Number, false),
    attr("Event", "returnValue", Kind::Bool, true),
    attr("Event", "srcElement", Kind::Null, false),
    attr("Event", "target", Kind::Null, false),
    attr("Event", "timeStamp", Kind::Number, false),
    attr("Event", "type", Kind::Str, false),
    attr("FocusEvent", "relatedTarget", Kind::Null, false),
    attr("FontFaceSetLoadEvent", "fontfaces", Kind::Array, false),
    attr("FormDataEvent", "formData", Kind::Null, false),
    attr("GamepadEvent", "gamepad", Kind::Null, false),
    attr("GPUUncapturedErrorEvent", "error", Kind::Null, false),
    attr("HashChangeEvent", "newURL", Kind::Str, false),
    attr("HashChangeEvent", "oldURL", Kind::Str, false),
    attr("HIDConnectionEvent", "device", Kind::Null, false),
    attr("HIDInputReportEvent", "data", Kind::Null, false),
    attr("HIDInputReportEvent", "device", Kind::Null, false),
    attr("HIDInputReportEvent", "reportId", Kind::Null, false),
    attr("IDBVersionChangeEvent", "dataLoss", Kind::Str, false),
    attr("IDBVersionChangeEvent", "dataLossMessage", Kind::Str, false),
    attr("IDBVersionChangeEvent", "newVersion", Kind::Null, false),
    attr("IDBVersionChangeEvent", "oldVersion", Kind::Number, false),
    attr("InputEvent", "data", Kind::Null, false),
    attr("InputEvent", "dataTransfer", Kind::Null, false),
    attr("InputEvent", "inputType", Kind::Str, false),
    attr("InputEvent", "isComposing", Kind::Bool, false),
    attr("InterestEvent", "source", Kind::Null, false),
    attr("KeyboardEvent", "altKey", Kind::Bool, false),
    attr("KeyboardEvent", "charCode", Kind::Number, false),
    attr("KeyboardEvent", "code", Kind::Str, false),
    attr("KeyboardEvent", "ctrlKey", Kind::Bool, false),
    attr("KeyboardEvent", "isComposing", Kind::Bool, false),
    attr("KeyboardEvent", "key", Kind::Str, false),
    attr("KeyboardEvent", "keyCode", Kind::Number, false),
    attr("KeyboardEvent", "location", Kind::Number, false),
    attr("KeyboardEvent", "metaKey", Kind::Bool, false),
    attr("KeyboardEvent", "repeat", Kind::Bool, false),
    attr("KeyboardEvent", "shiftKey", Kind::Bool, false),
    attr("MediaEncryptedEvent", "initData", Kind::Null, false),
    attr("MediaEncryptedEvent", "initDataType", Kind::Str, false),
    attr("MediaKeyMessageEvent", "message", Kind::Null, false),
    attr("MediaKeyMessageEvent", "messageType", Kind::Null, false),
    attr("MediaQueryListEvent", "matches", Kind::Bool, false),
    attr("MediaQueryListEvent", "media", Kind::Str, false),
    attr("MediaStreamEvent", "stream", Kind::Null, false),
    attr("MediaStreamTrackEvent", "track", Kind::Null, false),
    attr("MessageEvent", "data", Kind::Null, false),
    attr("MessageEvent", "lastEventId", Kind::Str, false),
    attr("MessageEvent", "origin", Kind::Str, false),
    attr("MessageEvent", "ports", Kind::Array, false),
    attr("MessageEvent", "source", Kind::Null, false),
    attr("MessageEvent", "userActivation", Kind::Null, false),
    attr("MIDIConnectionEvent", "port", Kind::Null, false),
    attr("MIDIMessageEvent", "data", Kind::Null, false),
    attr("MouseEvent", "altKey", Kind::Bool, false),
    attr("MouseEvent", "button", Kind::Number, false),
    attr("MouseEvent", "buttons", Kind::Number, false),
    attr("MouseEvent", "clientX", Kind::Number, false),
    attr("MouseEvent", "clientY", Kind::Number, false),
    attr("MouseEvent", "ctrlKey", Kind::Bool, false),
    attr("MouseEvent", "fromElement", Kind::Null, false),
    attr("MouseEvent", "layerX", Kind::Number, false),
    attr("MouseEvent", "layerY", Kind::Number, false),
    attr("MouseEvent", "metaKey", Kind::Bool, false),
    attr("MouseEvent", "movementX", Kind::Number, false),
    attr("MouseEvent", "movementY", Kind::Number, false),
    attr("MouseEvent", "offsetX", Kind::Number, false),
    attr("MouseEvent", "offsetY", Kind::Number, false),
    attr("MouseEvent", "pageX", Kind::Number, false),
    attr("MouseEvent", "pageY", Kind::Number, false),
    attr("MouseEvent", "relatedTarget", Kind::Null, false),
    attr("MouseEvent", "screenX", Kind::Number, false),
    attr("MouseEvent", "screenY", Kind::Number, false),
    attr("MouseEvent", "shiftKey", Kind::Bool, false),
    attr("MouseEvent", "toElement", Kind::Null, false),
    attr("MouseEvent", "x", Kind::Number, false),
    attr("MouseEvent", "y", Kind::Number, false),
    attr("NavigateEvent", "canIntercept", Kind::Null, false),
    attr("NavigateEvent", "destination", Kind::Null, false),
    attr("NavigateEvent", "downloadRequest", Kind::Null, false),
    attr("NavigateEvent", "formData", Kind::Null, false),
    attr("NavigateEvent", "hasUAVisualTransition", Kind::Null, false),
    attr("NavigateEvent", "hashChange", Kind::Null, false),
    attr("NavigateEvent", "info", Kind::Null, false),
    attr("NavigateEvent", "navigationType", Kind::Null, false),
    attr("NavigateEvent", "signal", Kind::Null, false),
    attr("NavigateEvent", "sourceElement", Kind::Null, false),
    attr("NavigateEvent", "userInitiated", Kind::Null, false),
    attr(
        "NavigationCurrentEntryChangeEvent",
        "from",
        Kind::Null,
        false,
    ),
    attr(
        "NavigationCurrentEntryChangeEvent",
        "navigationType",
        Kind::Null,
        false,
    ),
    attr(
        "OfflineAudioCompletionEvent",
        "renderedBuffer",
        Kind::Null,
        false,
    ),
    attr("PageRevealEvent", "viewTransition", Kind::Null, false),
    attr("PageSwapEvent", "activation", Kind::Null, false),
    attr("PageSwapEvent", "viewTransition", Kind::Null, false),
    attr("PageTransitionEvent", "persisted", Kind::Bool, false),
    attr(
        "PaymentMethodChangeEvent",
        "methodDetails",
        Kind::Null,
        false,
    ),
    attr("PaymentMethodChangeEvent", "methodName", Kind::Str, false),
    attr(
        "PictureInPictureEvent",
        "pictureInPictureWindow",
        Kind::Null,
        false,
    ),
    attr("PointerEvent", "altitudeAngle", Kind::Number, false),
    attr("PointerEvent", "azimuthAngle", Kind::Number, false),
    attr("PointerEvent", "height", Kind::Number, false),
    attr("PointerEvent", "isPrimary", Kind::Bool, false),
    attr("PointerEvent", "persistentDeviceId", Kind::Number, false),
    attr("PointerEvent", "pointerId", Kind::Number, false),
    attr("PointerEvent", "pointerType", Kind::Str, false),
    attr("PointerEvent", "pressure", Kind::Number, false),
    attr("PointerEvent", "tangentialPressure", Kind::Number, false),
    attr("PointerEvent", "tiltX", Kind::Number, false),
    attr("PointerEvent", "tiltY", Kind::Number, false),
    attr("PointerEvent", "twist", Kind::Number, false),
    attr("PointerEvent", "width", Kind::Number, false),
    attr("PopStateEvent", "hasUAVisualTransition", Kind::Bool, false),
    attr("PopStateEvent", "state", Kind::Null, false),
    attr(
        "PresentationConnectionAvailableEvent",
        "connection",
        Kind::Null,
        false,
    ),
    attr(
        "PresentationConnectionCloseEvent",
        "message",
        Kind::Null,
        false,
    ),
    attr(
        "PresentationConnectionCloseEvent",
        "reason",
        Kind::Null,
        false,
    ),
    attr("ProgressEvent", "lengthComputable", Kind::Bool, false),
    attr("ProgressEvent", "loaded", Kind::Number, false),
    attr("ProgressEvent", "total", Kind::Number, false),
    attr("PromiseRejectionEvent", "promise", Kind::Null, false),
    attr("PromiseRejectionEvent", "reason", Kind::Undefined, false),
    attr("RTCDataChannelEvent", "channel", Kind::Null, false),
    attr("RTCDTMFToneChangeEvent", "tone", Kind::Str, false),
    attr("RTCErrorEvent", "error", Kind::Null, false),
    attr(
        "RTCPeerConnectionIceErrorEvent",
        "address",
        Kind::Null,
        false,
    ),
    attr(
        "RTCPeerConnectionIceErrorEvent",
        "errorCode",
        Kind::Null,
        false,
    ),
    attr(
        "RTCPeerConnectionIceErrorEvent",
        "errorText",
        Kind::Null,
        false,
    ),
    attr(
        "RTCPeerConnectionIceErrorEvent",
        "hostCandidate",
        Kind::Null,
        false,
    ),
    attr("RTCPeerConnectionIceErrorEvent", "port", Kind::Null, false),
    attr("RTCPeerConnectionIceErrorEvent", "url", Kind::Null, false),
    attr("RTCPeerConnectionIceEvent", "candidate", Kind::Null, false),
    attr("RTCTrackEvent", "receiver", Kind::Null, false),
    attr("RTCTrackEvent", "streams", Kind::Null, false),
    attr("RTCTrackEvent", "track", Kind::Null, false),
    attr("RTCTrackEvent", "transceiver", Kind::Null, false),
    attr(
        "SecurityPolicyViolationEvent",
        "blockedURI",
        Kind::Str,
        false,
    ),
    attr(
        "SecurityPolicyViolationEvent",
        "columnNumber",
        Kind::Number,
        false,
    ),
    attr(
        "SecurityPolicyViolationEvent",
        "disposition",
        Kind::Str,
        false,
    ),
    attr(
        "SecurityPolicyViolationEvent",
        "documentURI",
        Kind::Str,
        false,
    ),
    attr(
        "SecurityPolicyViolationEvent",
        "effectiveDirective",
        Kind::Str,
        false,
    ),
    attr(
        "SecurityPolicyViolationEvent",
        "lineNumber",
        Kind::Number,
        false,
    ),
    attr(
        "SecurityPolicyViolationEvent",
        "originalPolicy",
        Kind::Str,
        false,
    ),
    attr("SecurityPolicyViolationEvent", "referrer", Kind::Str, false),
    attr("SecurityPolicyViolationEvent", "sample", Kind::Str, false),
    attr(
        "SecurityPolicyViolationEvent",
        "sourceFile",
        Kind::Str,
        false,
    ),
    attr(
        "SecurityPolicyViolationEvent",
        "statusCode",
        Kind::Number,
        false,
    ),
    attr(
        "SecurityPolicyViolationEvent",
        "violatedDirective",
        Kind::Str,
        false,
    ),
    attr("SensorErrorEvent", "error", Kind::Null, false),
    attr("SnapEvent", "snapTargetBlock", Kind::Null, false),
    attr("SnapEvent", "snapTargetInline", Kind::Null, false),
    attr("SpeechRecognitionErrorEvent", "error", Kind::Str, false),
    attr("SpeechRecognitionErrorEvent", "message", Kind::Str, false),
    attr("SpeechRecognitionEvent", "resultIndex", Kind::Number, false),
    attr("SpeechRecognitionEvent", "results", Kind::Null, false),
    attr("SpeechSynthesisErrorEvent", "error", Kind::Null, false),
    attr("SpeechSynthesisEvent", "charIndex", Kind::Null, false),
    attr("SpeechSynthesisEvent", "charLength", Kind::Null, false),
    attr("SpeechSynthesisEvent", "elapsedTime", Kind::Null, false),
    attr("SpeechSynthesisEvent", "name", Kind::Null, false),
    attr("SpeechSynthesisEvent", "utterance", Kind::Null, false),
    attr("StorageEvent", "key", Kind::Null, false),
    attr("StorageEvent", "newValue", Kind::Null, false),
    attr("StorageEvent", "oldValue", Kind::Null, false),
    attr("StorageEvent", "storageArea", Kind::Null, false),
    attr("StorageEvent", "url", Kind::Str, false),
    attr("SubmitEvent", "submitter", Kind::Null, false),
    attr(
        "TaskPriorityChangeEvent",
        "previousPriority",
        Kind::Str,
        false,
    ),
    attr("TextEvent", "data", Kind::Null, false),
    attr("TextUpdateEvent", "selectionEnd", Kind::Number, false),
    attr("TextUpdateEvent", "selectionStart", Kind::Number, false),
    attr("TextUpdateEvent", "text", Kind::Str, false),
    attr("TextUpdateEvent", "updateRangeEnd", Kind::Number, false),
    attr("TextUpdateEvent", "updateRangeStart", Kind::Number, false),
    attr("ToggleEvent", "newState", Kind::Str, false),
    attr("ToggleEvent", "oldState", Kind::Str, false),
    attr("ToggleEvent", "source", Kind::Null, false),
    attr("TouchEvent", "altKey", Kind::Bool, false),
    attr("TouchEvent", "changedTouches", Kind::Null, false),
    attr("TouchEvent", "ctrlKey", Kind::Bool, false),
    attr("TouchEvent", "metaKey", Kind::Bool, false),
    attr("TouchEvent", "shiftKey", Kind::Bool, false),
    attr("TouchEvent", "targetTouches", Kind::Null, false),
    attr("TouchEvent", "touches", Kind::Null, false),
    attr("TrackEvent", "track", Kind::Null, false),
    attr("TransitionEvent", "animation", Kind::Null, false),
    attr("TransitionEvent", "elapsedTime", Kind::Number, false),
    attr("TransitionEvent", "propertyName", Kind::Str, false),
    attr("TransitionEvent", "pseudoElement", Kind::Str, false),
    attr("TransitionEvent", "pseudoTarget", Kind::Null, false),
    attr("UIEvent", "detail", Kind::Number, false),
    attr("UIEvent", "pseudoTarget", Kind::Null, false),
    attr("UIEvent", "sourceCapabilities", Kind::Null, false),
    attr("UIEvent", "view", Kind::Null, false),
    attr("UIEvent", "which", Kind::Number, false),
    attr("USBConnectionEvent", "device", Kind::Null, false),
    attr("WebGLContextEvent", "statusMessage", Kind::Str, false),
    attr(
        "webkitSpeechRecognitionEvent",
        "resultIndex",
        Kind::Number,
        false,
    ),
    attr("webkitSpeechRecognitionEvent", "results", Kind::Null, false),
    attr("WheelEvent", "deltaMode", Kind::Number, false),
    attr("WheelEvent", "deltaX", Kind::Number, false),
    attr("WheelEvent", "deltaY", Kind::Number, false),
    attr("WheelEvent", "deltaZ", Kind::Number, false),
    attr("WheelEvent", "momentum", Kind::Bool, false),
    attr("WheelEvent", "wheelDelta", Kind::Number, false),
    attr("WheelEvent", "wheelDeltaX", Kind::Number, false),
    attr("WheelEvent", "wheelDeltaY", Kind::Number, false),
    attr(
        "WindowControlsOverlayGeometryChangeEvent",
        "titlebarAreaRect",
        Kind::Null,
        false,
    ),
    attr(
        "WindowControlsOverlayGeometryChangeEvent",
        "visible",
        Kind::Null,
        false,
    ),
    attr("XRInputSourceEvent", "frame", Kind::Null, false),
    attr("XRInputSourceEvent", "inputSource", Kind::Null, false),
    attr("XRInputSourcesChangeEvent", "added", Kind::Null, false),
    attr("XRInputSourcesChangeEvent", "removed", Kind::Null, false),
    attr("XRInputSourcesChangeEvent", "session", Kind::Null, false),
    attr("XRLayerEvent", "layer", Kind::Null, false),
    attr("XRReferenceSpaceEvent", "referenceSpace", Kind::Null, false),
    attr("XRReferenceSpaceEvent", "transform", Kind::Null, false),
    attr("XRSessionEvent", "session", Kind::Null, false),
    attr("XRVisibilityMaskChangeEvent", "eye", Kind::Null, false),
    attr("XRVisibilityMaskChangeEvent", "index", Kind::Null, false),
    attr("XRVisibilityMaskChangeEvent", "indices", Kind::Null, false),
    attr("XRVisibilityMaskChangeEvent", "session", Kind::Null, false),
    attr("XRVisibilityMaskChangeEvent", "vertices", Kind::Null, false),
];

fn default_value(scope: &mut Scope<'_>, kind: Kind) -> Value {
    match kind {
        Kind::Number => Value::int(0),
        Kind::Bool => Value::boolean(false),
        Kind::Str => scope.string(""),
        Kind::Undefined => Value::undefined(),
        Kind::Array => {
            let array = scope.new_array();
            let _ = scope.freeze(&array);
            array
        }
        Kind::Null => Value::null(),
    }
}

fn illegal(scope: &mut Scope<'_>) -> Value {
    scope.type_error("Illegal invocation")
}

fn plain_receiver(scope: &mut Scope<'_>, this: &Value) -> JsResult<()> {
    if !this.is_object() {
        return Err(illegal(scope));
    }
    let key = scope.string("constructor");
    match scope.own_property(this, &key)? {
        None => Ok(()),
        Some(_) => Err(illegal(scope)),
    }
}

fn index_of(scope: &mut Scope<'_>, data: &[Value]) -> JsResult<&'static Attr> {
    let index = match data.first() {
        Some(value) => scope.to_int32(value)?,
        None => -1,
    };
    usize::try_from(index)
        .ok()
        .and_then(|i| ATTRS.get(i))
        .ok_or_else(|| illegal(scope))
}

fn attr_get(scope: &mut Scope<'_>, this: &Value, _: &[Value], data: &[Value]) -> JsResult {
    let attr = index_of(scope, data)?;
    let state = ffi::event_state(scope, this);
    let own = match &state {
        Some(state) => {
            let key = scope.string(attr.name);
            scope.own_property(state, &key)?
        }
        None => {
            plain_receiver(scope, this)?;
            None
        }
    };
    match own {
        Some(desc) if desc.accessor => {
            if scope.is_function(&desc.getter) {
                scope.call(&desc.getter, this, &[])
            } else {
                Ok(Value::undefined())
            }
        }
        Some(desc) => Ok(desc.value),
        None => Ok(default_value(scope, attr.kind)),
    }
}

fn attr_set(scope: &mut Scope<'_>, this: &Value, args: &[Value], data: &[Value]) -> JsResult {
    let attr = index_of(scope, data)?;
    let Some(state) = ffi::event_state(scope, this) else {
        return Err(illegal(scope));
    };
    let mut value = arg(args, 0);
    if attr.kind == Kind::Str {
        value = scope.to_string_value(&value)?;
    }
    let _ = scope.set(&state, attr.name, value);
    Ok(Value::undefined())
}

pub(crate) fn install(scope: &mut Scope<'_>, global: &Value) {
    let mut iface = "";
    let mut proto = Value::undefined();
    for (index, attr) in ATTRS.iter().enumerate() {
        if iface != attr.iface {
            iface = attr.iface;
            let ctor = get(scope, global, iface);
            proto = if scope.is_function(&ctor) {
                get(scope, &ctor, "prototype")
            } else {
                Value::undefined()
            };
        }
        if !proto.is_object() {
            continue;
        }
        let key = scope.string(attr.name);
        if !matches!(scope.own_property(&proto, &key), Ok(None)) {
            continue;
        }
        let data = [Value::int(index as i32)];
        let getter = scope.bound_function(&format!("get {}", attr.name), 0, attr_get, &data);
        let setter = attr
            .writable
            .then(|| scope.bound_function(&format!("set {}", attr.name), 1, attr_set, &data));
        let _ = scope.define_accessor(
            &proto,
            attr.name,
            Some(&getter),
            setter.as_ref(),
            ENUMERABLE_CONFIGURABLE,
        );
    }
}
