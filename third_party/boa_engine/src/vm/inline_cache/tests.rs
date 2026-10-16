use boa_gc::Gc;
use boa_parser::Source;

use crate::{
    Context, JsObject, JsResult, JsValue,
    builtins::{OrdinaryObject, function::OrdinaryFunction},
    js_string,
    object::{
        ObjectInitializer, internal_methods::InternalMethodPropertyContext,
        shape::slot::SlotAttributes,
    },
    property::{Attribute, PropertyDescriptor, PropertyKey},
    vm::CodeBlock,
};

#[test]
fn get_own_property_internal_method() {
    let context = &mut Context::default();

    let o = context
        .intrinsics()
        .templates()
        .ordinary_object()
        .create(OrdinaryObject, Vec::default());

    let property: PropertyKey = js_string!("prop").into();
    let value = 100;

    o.set(property.clone(), value, true, context)
        .expect("should not fail");

    let context = &mut InternalMethodPropertyContext::new(context);

    assert_eq!(context.slot().index, 0);
    assert_eq!(context.slot().attributes, SlotAttributes::empty());

    o.__get_own_property__(&property, context)
        .expect("should not fail");

    assert!(
        !context.slot().in_prototype(),
        "Since it's an owned property, the prototype bit should not be set"
    );

    assert!(
        context.slot().is_cacheable(),
        "Since it's an owned property, this should be cacheable"
    );

    let shape = o.borrow().shape().clone();

    let slot = shape.lookup(&property);

    assert!(slot.is_some(), "the property should be found in the object");

    let slot = slot.expect("the property should be found in the object");

    assert_eq!(context.slot().index, slot.index);
}

#[test]
fn get_internal_method() {
    let context = &mut Context::default();

    let o = context
        .intrinsics()
        .templates()
        .ordinary_object()
        .create(OrdinaryObject, Vec::default());

    let property: PropertyKey = js_string!("prop").into();
    let value = 100;

    o.set(property.clone(), value, true, context)
        .expect("should not fail");

    let context = &mut InternalMethodPropertyContext::new(context);

    assert_eq!(context.slot().index, 0);
    assert_eq!(context.slot().attributes, SlotAttributes::empty());

    o.__get__(&property, o.clone().into(), context)
        .expect("should not fail");

    assert!(
        !context.slot().in_prototype(),
        "Since it's an owned property, the prototype bit should not be set"
    );

    assert!(
        context.slot().is_cacheable(),
        "Since it's an owned property, this should be cacheable"
    );

    let shape = o.borrow().shape().clone();

    let slot = shape.lookup(&property);

    assert!(slot.is_some(), "the property should be found in the object");

    let slot = slot.expect("the property should be found in the object");

    assert_eq!(context.slot().index, slot.index);
}

#[test]
fn get_internal_method_in_prototype() {
    let context = &mut Context::default();

    let o = context
        .intrinsics()
        .templates()
        .ordinary_object()
        .create(OrdinaryObject, Vec::default());

    let property: PropertyKey = js_string!("prop").into();
    let value = 100;

    let prototype = context.intrinsics().constructors().object().prototype();

    prototype
        .set(property.clone(), value, true, context)
        .expect("should not fail");

    let context = &mut InternalMethodPropertyContext::new(context);

    assert_eq!(context.slot().index, 0);
    assert_eq!(context.slot().attributes, SlotAttributes::empty());

    o.__get__(&property, o.clone().into(), context)
        .expect("should not fail");

    assert!(
        context.slot().in_prototype(),
        "Since it's an prototype property, the prototype bit should not be set"
    );

    assert!(
        context.slot().is_cacheable(),
        "Since it's an prototype property, this should be cacheable"
    );

    let shape = prototype.borrow().shape().clone();

    let slot = shape.lookup(&property);

    assert!(slot.is_some(), "the property should be found in the object");

    let slot = slot.expect("the property should be found in the object");

    assert_eq!(context.slot().index, slot.index);
}

#[test]
fn define_own_property_internal_method_non_existent_property() {
    let context = &mut Context::default();

    let o = context
        .intrinsics()
        .templates()
        .ordinary_object()
        .create(OrdinaryObject, Vec::default());

    let property: PropertyKey = js_string!("prop").into();
    let value = 100;

    o.set(property.clone(), value, true, context)
        .expect("should not fail");

    let context = &mut InternalMethodPropertyContext::new(context);

    assert_eq!(context.slot().index, 0);
    assert_eq!(context.slot().attributes, SlotAttributes::empty());

    o.__define_own_property__(
        &property,
        PropertyDescriptor::builder()
            .value(value)
            .writable(true)
            .configurable(true)
            .enumerable(true)
            .build(),
        context,
    )
    .expect("should not fail");

    assert!(
        !context.slot().in_prototype(),
        "Since it's an owned property, the prototype bit should not be set"
    );

    assert!(
        context.slot().is_cacheable(),
        "Since it's an owned property, this should be cacheable"
    );

    let shape = o.borrow().shape().clone();

    let slot = shape.lookup(&property);

    assert!(slot.is_some(), "the property should be found in the object");

    let slot = slot.expect("the property should be found in the object");

    assert_eq!(context.slot().index, slot.index);
}

#[test]
fn define_own_property_internal_method_existing_property_property() {
    let context = &mut Context::default();

    let o = context
        .intrinsics()
        .templates()
        .ordinary_object()
        .create(OrdinaryObject, Vec::default());

    let property: PropertyKey = js_string!("prop").into();
    let value = 100;

    o.set(property.clone(), value, true, context)
        .expect("should not fail");

    o.__define_own_property__(
        &property,
        PropertyDescriptor::builder()
            .value(value)
            .writable(true)
            .configurable(true)
            .enumerable(true)
            .build(),
        &mut context.into(),
    )
    .expect("should not fail");

    let context = &mut InternalMethodPropertyContext::new(context);

    assert_eq!(context.slot().index, 0);
    assert_eq!(context.slot().attributes, SlotAttributes::empty());

    o.__define_own_property__(
        &property,
        PropertyDescriptor::builder()
            .value(value + 100)
            .writable(true)
            .configurable(true)
            .enumerable(true)
            .build(),
        context,
    )
    .expect("should not fail");

    assert!(
        !context.slot().in_prototype(),
        "Since it's an owned property, the prototype bit should not be set"
    );

    assert!(
        context.slot().is_cacheable(),
        "Since it's an owned property, this should be cacheable"
    );

    let shape = o.borrow().shape().clone();

    let slot = shape.lookup(&property);

    assert!(slot.is_some(), "the property should be found in the object");

    let slot = slot.expect("the property should be found in the object");

    assert_eq!(context.slot().index, slot.index);
}

#[test]
fn set_internal_method() {
    let context = &mut Context::default();

    let o = context
        .intrinsics()
        .templates()
        .ordinary_object()
        .create(OrdinaryObject, Vec::default());

    let property: PropertyKey = js_string!("prop").into();
    let value = 100;

    o.set(property.clone(), value, true, context)
        .expect("should not fail");

    let context = &mut InternalMethodPropertyContext::new(context);

    assert_eq!(context.slot().index, 0);
    assert_eq!(context.slot().attributes, SlotAttributes::empty());

    o.__set__(property.clone(), value.into(), o.clone().into(), context)
        .expect("should not fail");

    assert!(
        !context.slot().in_prototype(),
        "Since it's an owned property, the prototype bit should not be set"
    );

    assert!(
        context.slot().is_cacheable(),
        "Since it's an owned property, this should be cacheable"
    );

    let shape = o.borrow().shape().clone();

    let slot = shape.lookup(&property);

    assert!(slot.is_some(), "the property should be found in the object");

    let slot = slot.expect("the property should be found in the object");

    assert_eq!(context.slot().index, slot.index);
}

fn get_codeblock(value: &JsValue) -> Option<(JsObject, Gc<CodeBlock>)> {
    let object = value.as_object()?.clone();
    let code = object.downcast_ref::<OrdinaryFunction>()?.code.clone();

    Some((object, code))
}

#[test]
fn set_property_by_name_set_inline_cache_on_property_load() -> JsResult<()> {
    let context = &mut Context::default();
    let function = context.eval(Source::from_bytes("(function (o) { return o.test; })"))?;
    let (function, code) = get_codeblock(&function).unwrap();

    assert_eq!(code.ic.len(), 1);
    assert_eq!(code.ic[0].entries.borrow().len(), 0);

    let o = ObjectInitializer::new(context)
        .property(js_string!("test"), 0, Attribute::all())
        .build();
    let o_shape = o.borrow().shape().clone();

    function.call(&JsValue::undefined(), &[o.clone().into()], context)?;

    assert_eq!(code.ic[0].entries.borrow().len(), 1);
    assert_eq!(
        code.ic[0].entries.borrow()[0]
            .shape
            .upgrade()
            .unwrap()
            .to_addr_usize(),
        o_shape.to_addr_usize()
    );

    Ok(())
}

#[test]
fn get_property_by_name_set_inline_cache_on_property_load() -> JsResult<()> {
    let context = &mut Context::default();
    let function = context.eval(Source::from_bytes("(function (o) { o.test = 30; })"))?;
    let (function, code) = get_codeblock(&function).unwrap();

    assert_eq!(code.ic.len(), 1);
    assert_eq!(code.ic[0].entries.borrow().len(), 0);

    let o = ObjectInitializer::new(context)
        .property(js_string!("test"), 0, Attribute::all())
        .build();
    let o_shape = o.borrow().shape().clone();

    function.call(&JsValue::undefined(), &[o.clone().into()], context)?;

    assert_eq!(code.ic[0].entries.borrow().len(), 1);
    assert_eq!(
        code.ic[0].entries.borrow()[0]
            .shape
            .upgrade()
            .unwrap()
            .to_addr_usize(),
        o_shape.to_addr_usize()
    );

    Ok(())
}

#[test]
fn test_polymorphic_inline_cache() -> JsResult<()> {
    let context = &mut Context::default();
    let function = context.eval(Source::from_bytes("(function (o) { return o.test; })"))?;
    let (function, code) = get_codeblock(&function).unwrap();

    assert_eq!(code.ic.len(), 1);
    assert_eq!(code.ic[0].entries.borrow().len(), 0);
    assert!(!code.ic[0].megamorphic.get());

    let shapes = vec![
        ObjectInitializer::new(context)
            .property(js_string!("test"), 1, Attribute::all())
            .build(),
        ObjectInitializer::new(context)
            .property(js_string!("a"), 2, Attribute::all())
            .property(js_string!("test"), 3, Attribute::all())
            .build(),
        ObjectInitializer::new(context)
            .property(js_string!("b"), 4, Attribute::all())
            .property(js_string!("test"), 5, Attribute::all())
            .build(),
        ObjectInitializer::new(context)
            .property(js_string!("c"), 6, Attribute::all())
            .property(js_string!("test"), 7, Attribute::all())
            .build(),
    ];

    for o in &shapes {
        function.call(&JsValue::undefined(), &[o.clone().into()], context)?;
    }

    assert_eq!(code.ic[0].entries.borrow().len(), 4);
    assert!(!code.ic[0].megamorphic.get());

    Ok(())
}

#[test]
fn test_megamorphic_inline_cache() -> JsResult<()> {
    let context = &mut Context::default();
    let function = context.eval(Source::from_bytes("(function (o) { return o.test; })"))?;
    let (function, code) = get_codeblock(&function).unwrap();

    let shapes = vec![
        ObjectInitializer::new(context)
            .property(js_string!("test"), 1, Attribute::all())
            .build(),
        ObjectInitializer::new(context)
            .property(js_string!("a"), 1, Attribute::all())
            .property(js_string!("test"), 1, Attribute::all())
            .build(),
        ObjectInitializer::new(context)
            .property(js_string!("b"), 1, Attribute::all())
            .property(js_string!("test"), 1, Attribute::all())
            .build(),
        ObjectInitializer::new(context)
            .property(js_string!("c"), 1, Attribute::all())
            .property(js_string!("test"), 1, Attribute::all())
            .build(),
        ObjectInitializer::new(context)
            .property(js_string!("d"), 1, Attribute::all())
            .property(js_string!("test"), 1, Attribute::all())
            .build(),
    ];

    for o in &shapes {
        function.call(&JsValue::undefined(), &[o.clone().into()], context)?;
    }

    assert_eq!(code.ic[0].entries.borrow().len(), 0);
    assert!(code.ic[0].megamorphic.get());

    // Regression check: repeated miss should remain empty
    let o6 = ObjectInitializer::new(context)
        .property(js_string!("e"), 1, Attribute::all())
        .property(js_string!("test"), 1, Attribute::all())
        .build();
    function.call(&JsValue::undefined(), &[o6.clone().into()], context)?;
    assert_eq!(code.ic[0].entries.borrow().len(), 0);
    assert!(code.ic[0].megamorphic.get());

    Ok(())
}

#[test]
fn prototype_slot_not_served_after_prototype_property_deleted() -> JsResult<()> {
    let context = &mut Context::default();
    let result = context.eval(Source::from_bytes(
        r"
        function readB(o) { return o.b; }
        Object.defineProperty(Object.prototype, 'a', { value: 'A', configurable: true });
        Object.defineProperty(Object.prototype, 'b', { value: 'B', configurable: true });
        Object.defineProperty(Object.prototype, 'c', { value: 'C', configurable: true });
        const cached = readB({}) + readB({});
        delete Object.prototype.a;
        const shifted = readB({});
        delete Object.prototype.b;
        delete Object.prototype.c;
        cached + shifted + String(readB({}));
        ",
    ))?;

    assert_eq!(result, JsValue::from(js_string!("BBBundefined")));

    Ok(())
}

#[test]
fn global_prototype_slot_not_served_after_prototype_property_deleted() -> JsResult<()> {
    let context = &mut Context::default();
    let result = context.eval(Source::from_bytes(
        r"
        Object.defineProperty(Object.prototype, 'a', { value: 'A', configurable: true });
        Object.defineProperty(Object.prototype, 'gb', { value: 'B', configurable: true });
        Object.defineProperty(Object.prototype, 'c', { value: 'C', configurable: true });
        function readGb() { return gb; }
        const cached = readGb() + readGb();
        delete Object.prototype.a;
        const shifted = readGb();
        delete Object.prototype.gb;
        delete Object.prototype.c;
        let gone;
        try { readGb(); gone = 'found'; } catch (e) { gone = e.name; }
        cached + shifted + gone;
        ",
    ))?;

    assert_eq!(result, JsValue::from(js_string!("BBBReferenceError")));

    Ok(())
}

#[test]
fn prototype_slot_not_served_after_prototype_property_becomes_accessor() -> JsResult<()> {
    let context = &mut Context::default();
    let result = context.eval(Source::from_bytes(
        r"
        function readB(o) { return o.b; }
        Object.defineProperty(Object.prototype, 'a', { value: 'A', configurable: true });
        Object.defineProperty(Object.prototype, 'b', { value: 'B', configurable: true });
        readB({});
        readB({});
        Object.defineProperty(Object.prototype, 'a', { get() { return 'getter'; }, configurable: true });
        const shifted = readB({});
        delete Object.prototype.a;
        delete Object.prototype.b;
        shifted;
        ",
    ))?;

    assert_eq!(result, JsValue::from(js_string!("B")));

    Ok(())
}

#[test]
fn prototype_slot_not_cached_when_getter_reshapes_prototype() -> JsResult<()> {
    let context = &mut Context::default();
    let result = context.eval(Source::from_bytes(
        r"
        const P = {};
        for (const k of ['a', 'x', 'y']) {
            Object.defineProperty(P, k, { value: k, configurable: true });
        }
        let first = true;
        Object.defineProperty(P, 'b', {
            get() {
                if (first) {
                    first = false;
                    delete P.a;
                    delete P.x;
                    delete P.y;
                }
                return 'B';
            },
            configurable: true,
        });
        const o = Object.create(P);
        function readB(o) { return o.b; }
        readB(o) + readB(o) + readB(o);
        ",
    ))?;

    assert_eq!(result, JsValue::from(js_string!("BBB")));

    Ok(())
}

#[test]
fn prototype_slot_not_cached_when_setter_reshapes_prototype() -> JsResult<()> {
    let context = &mut Context::default();
    let result = context.eval(Source::from_bytes(
        r"
        const P = {};
        for (const k of ['a', 'x', 'y']) {
            Object.defineProperty(P, k, { value: k, configurable: true });
        }
        let first = true;
        const seen = [];
        Object.defineProperty(P, 'b', {
            set(v) {
                seen.push(v);
                if (first) {
                    first = false;
                    delete P.a;
                }
            },
            configurable: true,
        });
        const o = Object.create(P);
        function writeB(o, v) { o.b = v; }
        writeB(o, 1);
        writeB(o, 2);
        writeB(o, 3);
        seen.join();
        ",
    ))?;

    assert_eq!(result, JsValue::from(js_string!("1,2,3")));

    Ok(())
}

#[test]
fn own_slot_not_served_after_unique_shape_attribute_change() -> JsResult<()> {
    let context = &mut Context::default();
    let result = context.eval(Source::from_bytes(
        r"
        function writeW(o, v) { o.w = v; }
        Object.prototype.w = 1;
        writeW(Object.prototype, 2);
        writeW(Object.prototype, 2);
        Object.defineProperty(Object.prototype, 'w', { writable: false });
        writeW(Object.prototype, 3);
        const w = Object.prototype.w;
        delete Object.prototype.w;
        w;
        ",
    ))?;

    assert_eq!(result, JsValue::from(2));

    Ok(())
}
