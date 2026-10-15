//! Class decorators and auto-accessors.
//!
//! A class with decorators or auto-accessors is defined in four steps, which follow
//! `ClassDefinitionEvaluation` of the [decorators proposal][spec]:
//!  - [`CreateClassDecorations`] starts a [`ClassDecorations`] record for the class,
//!  - [`AddClassElement`] adds each class element once its decorators, name and value have
//!    been evaluated,
//!  - [`DecorateClass`] applies the decorators of the elements and of the class, and defines
//!    the elements,
//!  - [`InitializeStaticElements`] runs the static initializers, fields and blocks.
//!
//! [spec]: https://arai-a.github.io/ecma262-compare/?pr=2417&id=sec-runtime-semantics-classdefinitionevaluation

use crate::{
    Context, JsArgs, JsData, JsExpect, JsObject, JsResult, JsString, JsSymbol, JsValue,
    builtins::{
        OrdinaryObject,
        function::{ClassFieldDefinition, OrdinaryFunction, set_function_name},
    },
    js_error, js_str, js_string,
    native_function::NativeFunction,
    object::{FunctionObjectBuilder, PrivateElement, PrivateName},
    property::{PropertyDescriptor, PropertyKey},
    vm::opcode::{IndexOperand, Operation, RegisterOperand},
};
use boa_gc::{Finalize, Gc, GcRefCell, Trace};

/// The flags operand of [`AddClassElement`]: an element kind, optionally combined with
/// [`STATIC`](Self::STATIC), [`PRIVATE`](Self::PRIVATE) and
/// [`ANONYMOUS_FUNCTION`](Self::ANONYMOUS_FUNCTION).
pub(crate) struct ClassElementFlags;

impl ClassElementFlags {
    /// A method; its value is the method.
    pub(crate) const METHOD: u32 = 0;
    /// A getter; its value is the getter.
    pub(crate) const GETTER: u32 = 1;
    /// A setter; its value is the setter.
    pub(crate) const SETTER: u32 = 2;
    /// A field; its value is the initializer.
    pub(crate) const FIELD: u32 = 3;
    /// An auto-accessor; its value is the initializer.
    pub(crate) const ACCESSOR: u32 = 4;
    /// A static block; its value is the body.
    pub(crate) const STATIC_BLOCK: u32 = 5;
    const KIND: u32 = 0b111;
    /// The element is static.
    pub(crate) const STATIC: u32 = 1 << 3;
    /// The element has a private name; its key is the description of the name.
    pub(crate) const PRIVATE: u32 = 1 << 4;
    /// The initializer is an anonymous function definition, named after the element.
    pub(crate) const ANONYMOUS_FUNCTION: u32 = 1 << 5;
}

/// The `[[Kind]]` of a class element.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Trace, Finalize)]
#[boa_gc(empty_trace)]
enum ElementKind {
    Method,
    Getter,
    Setter,
    Field,
    Accessor,
    StaticBlock,
}

impl ElementKind {
    const fn from_flags(flags: u32) -> Self {
        match flags & ClassElementFlags::KIND {
            ClassElementFlags::GETTER => Self::Getter,
            ClassElementFlags::SETTER => Self::Setter,
            ClassElementFlags::FIELD => Self::Field,
            ClassElementFlags::ACCESSOR => Self::Accessor,
            ClassElementFlags::STATIC_BLOCK => Self::StaticBlock,
            _ => Self::Method,
        }
    }

    /// The `kind` of a decorator context object.
    const fn name(self) -> JsString {
        match self {
            Self::Method => js_string!("method"),
            Self::Getter => js_string!("getter"),
            Self::Setter => js_string!("setter"),
            Self::Field => js_string!("field"),
            Self::Accessor => js_string!("accessor"),
            Self::StaticBlock => js_string!("class"),
        }
    }

    const fn is_field_or_accessor(self) -> bool {
        matches!(self, Self::Field | Self::Accessor)
    }
}

/// The name of a class element: a property key, as a string or symbol, or a private name.
#[derive(Debug, Clone, Trace, Finalize)]
pub enum ElementName {
    /// A property key.
    Public(JsValue),
    /// A private name.
    Private(PrivateName),
}

impl ElementName {
    /// The key `SetFunctionName` names a function of the element with.
    fn function_key(&self, context: &mut Context) -> JsResult<PropertyKey> {
        match self {
            Self::Public(key) => key.to_property_key(context),
            Self::Private(name) => Ok(js_string!(js_str!("#"), name.description()).into()),
        }
    }

    /// The `name` of a decorator context object.
    fn context_name(&self) -> JsValue {
        match self {
            Self::Public(key) => key.clone(),
            Self::Private(name) => js_string!(js_str!("#"), name.description()).into(),
        }
    }
}

/// A `DecoratorDefinition` Record: a decorator and the `this` value it is called with.
#[derive(Debug, Clone, Trace, Finalize)]
struct Decorator {
    function: JsValue,
    receiver: JsValue,
}

/// A list of initializers that `addInitializer` of a decorator context object appends to.
type Initializers = Gc<GcRefCell<Vec<JsObject>>>;

/// A `ClassElementDefinition` or `ClassStaticBlockDefinition` Record.
#[derive(Debug, Trace, Finalize)]
struct ElementRecord {
    kind: ElementKind,
    is_static: bool,
    name: ElementName,
    /// The method, getter or setter; the initializer of a field or auto-accessor; the body of a
    /// static block.
    value: Option<JsObject>,
    getter: Option<JsObject>,
    setter: Option<JsObject>,
    /// The `[[BackingStorageKey]]` of an auto-accessor.
    storage: Option<PrivateName>,
    /// The initializers returned by the decorators of a field or auto-accessor, in call order.
    initializers: Vec<JsObject>,
    extra_initializers: Initializers,
    decorators: Vec<Decorator>,
    names_anonymous_function: bool,
}

impl ElementRecord {
    /// The field definition that initializes this field or auto-accessor.
    fn field(&self) -> DecoratedField {
        DecoratedField {
            name: self
                .storage
                .clone()
                .map_or_else(|| self.name.clone(), ElementName::Private),
            initializer: self.value.clone(),
            function_name: match &self.name {
                ElementName::Public(key) if self.names_anonymous_function => Some(key.clone()),
                _ => None,
            },
            initializers: self.initializers.clone(),
            extra_initializers: self.extra_initializers.borrow().clone(),
        }
    }
}

/// A field or the storage of an auto-accessor, of a class with decorators.
#[derive(Debug, Trace, Finalize)]
pub struct DecoratedField {
    name: ElementName,
    initializer: Option<JsObject>,
    function_name: Option<JsValue>,
    initializers: Vec<JsObject>,
    extra_initializers: Vec<JsObject>,
}

impl DecoratedField {
    /// Abstract operation `InitializeFieldOrAccessor ( receiver, elementRecord )`
    ///
    /// The field's own initializer runs first; each initializer its decorators returned
    /// receives the value so far.
    pub(crate) fn initialize(&self, receiver: &JsObject, context: &mut Context) -> JsResult<()> {
        let this = JsValue::from(receiver.clone());
        let mut value = match &self.initializer {
            Some(initializer) => initializer.call(&this, &[], context)?,
            None => JsValue::undefined(),
        };
        if let Some(key) = &self.function_name
            && let Some(function) = value.as_object()
        {
            set_function_name(&function, &key.to_property_key(context)?, None, context)?;
        }
        for initializer in &self.initializers {
            value = initializer.call(&this, &[value], context)?;
        }
        match &self.name {
            ElementName::Private(name) => receiver.private_field_add(name, value, context)?,
            ElementName::Public(key) => {
                receiver.create_data_property_or_throw(
                    key.to_property_key(context)?,
                    value,
                    context,
                )?;
            }
        }
        for initializer in &self.extra_initializers {
            initializer.call(&this, &[], context)?;
        }
        Ok(())
    }
}

/// The class being defined, with its elements, until [`InitializeStaticElements`] completes it.
#[derive(Debug, Trace, Finalize, JsData)]
pub(crate) struct ClassDecorations {
    class: JsObject,
    prototype: JsObject,
    class_name: JsString,
    decorators: Vec<Decorator>,
    metadata: Option<JsObject>,
    elements: Vec<ElementRecord>,
    static_method_initializers: Initializers,
    instance_method_initializers: Initializers,
    class_initializers: Initializers,
}

/// Reads the decorators an array of decorator and `this` value pairs holds.
fn read_decorators(array: &JsValue, context: &mut Context) -> JsResult<Vec<Decorator>> {
    let Some(array) = array.as_object() else {
        return Ok(Vec::new());
    };
    let length = array.length_of_array_like(context)?;
    let mut decorators = Vec::with_capacity((length / 2) as usize);
    for index in (0..length).step_by(2) {
        decorators.push(Decorator {
            function: array.get(index, context)?,
            receiver: array.get(index + 1, context)?,
        });
    }
    Ok(decorators)
}

/// Sets the `[[HomeObject]]` of a method or initializer compiled with the class.
fn set_home_object(function: &JsObject, home: &JsObject) -> JsResult<()> {
    function
        .downcast_mut::<OrdinaryFunction>()
        .js_expect("class element must be a function object")?
        .set_home_object(home.clone());
    Ok(())
}

/// Abstract operations `MakeAutoAccessorGetter` and `MakeAutoAccessorSetter`.
fn make_auto_accessor(
    name: &ElementName,
    storage: &PrivateName,
    context: &mut Context,
) -> JsResult<(JsObject, JsObject)> {
    let getter: JsObject = FunctionObjectBuilder::new(
        context.realm(),
        NativeFunction::from_copy_closure_with_captures(
            |this, _, storage, context| {
                let object = this.as_object().ok_or_else(
                    || js_error!(TypeError: "auto-accessor getter called on a non-object"),
                )?;
                object.private_get(storage, context)
            },
            storage.clone(),
        ),
    )
    .length(0)
    .build()
    .into();
    let setter: JsObject = FunctionObjectBuilder::new(
        context.realm(),
        NativeFunction::from_copy_closure_with_captures(
            |this, args, storage, context| {
                let object = this.as_object().ok_or_else(
                    || js_error!(TypeError: "auto-accessor setter called on a non-object"),
                )?;
                object.private_set(storage, args.get_or_undefined(0).clone(), context)?;
                Ok(JsValue::undefined())
            },
            storage.clone(),
        ),
    )
    .length(1)
    .build()
    .into();
    let key = name.function_key(context)?;
    set_function_name(&getter, &key, Some(js_str!("get")), context)?;
    set_function_name(&setter, &key, Some(js_str!("set")), context)?;
    Ok((getter, setter))
}

/// The captures of an `addInitializer` function.
#[derive(Trace, Finalize)]
struct AddInitializer {
    initializers: Initializers,
    finished: Gc<GcRefCell<bool>>,
}

/// Abstract operation `CreateAddInitializerFunction ( initializers, decorationState )`
fn create_add_initializer_function(
    initializers: &Initializers,
    finished: &Gc<GcRefCell<bool>>,
    context: &mut Context,
) -> JsObject {
    FunctionObjectBuilder::new(
        context.realm(),
        NativeFunction::from_copy_closure_with_captures(
            |_, args, captures, _| {
                // 1. If decorationState.[[Finished]] is true, throw a TypeError exception.
                if *captures.finished.borrow() {
                    return Err(js_error!(
                        TypeError: "addInitializer called after its decorator returned"
                    ));
                }
                // 2. If IsCallable(initializer) is false, throw a TypeError exception.
                let initializer = args
                    .get_or_undefined(0)
                    .as_callable()
                    .ok_or_else(|| js_error!(TypeError: "initializer must be a function"))?;
                // 3. Append initializer to initializers.
                captures.initializers.borrow_mut().push(initializer);
                Ok(JsValue::undefined())
            },
            AddInitializer {
                initializers: initializers.clone(),
                finished: finished.clone(),
            },
        ),
    )
    .name(js_string!("addInitializer"))
    .length(1)
    .build()
    .into()
}

/// Abstract operation `CreateDecoratorAccessObject ( kind, name )`
fn create_decorator_access_object(
    kind: ElementKind,
    name: &ElementName,
    context: &mut Context,
) -> JsResult<JsObject> {
    // 1. Let accessObj be OrdinaryObjectCreate(%Object.prototype%).
    let access = JsObject::with_object_proto(context.intrinsics());

    // 2. If kind is field, method, accessor, or getter, then
    if !matches!(kind, ElementKind::Setter) {
        let get = FunctionObjectBuilder::new(
            context.realm(),
            NativeFunction::from_copy_closure_with_captures(
                |_, args, name, context| {
                    let object = args.get_or_undefined(0).as_object().ok_or_else(
                        || js_error!(TypeError: "decorator access target must be an object"),
                    )?;
                    match name {
                        ElementName::Public(key) => {
                            object.get(key.to_property_key(context)?, context)
                        }
                        ElementName::Private(name) => object.private_get(name, context),
                    }
                },
                name.clone(),
            ),
        )
        .name(js_string!())
        .length(1)
        .build();
        access.create_data_property_or_throw(js_string!("get"), get, context)?;
    }

    // 3. If kind is field, accessor, or setter, then
    if matches!(
        kind,
        ElementKind::Field | ElementKind::Accessor | ElementKind::Setter
    ) {
        let set = FunctionObjectBuilder::new(
            context.realm(),
            NativeFunction::from_copy_closure_with_captures(
                |_, args, name, context| {
                    let object = args.get_or_undefined(0).as_object().ok_or_else(
                        || js_error!(TypeError: "decorator access target must be an object"),
                    )?;
                    let value = args.get_or_undefined(1).clone();
                    match name {
                        ElementName::Public(key) => {
                            object.set(key.to_property_key(context)?, value, true, context)?;
                        }
                        ElementName::Private(name) => object.private_set(name, value, context)?,
                    }
                    Ok(JsValue::undefined())
                },
                name.clone(),
            ),
        )
        .name(js_string!())
        .length(2)
        .build();
        access.create_data_property_or_throw(js_string!("set"), set, context)?;
    }

    // 4. Let hasClosure be a new Abstract Closure with parameters (obj) that captures name:
    let has = FunctionObjectBuilder::new(
        context.realm(),
        NativeFunction::from_copy_closure_with_captures(
            |_, args, name, context| {
                let object = args.get_or_undefined(0).as_object().ok_or_else(
                    || js_error!(TypeError: "decorator access target must be an object"),
                )?;
                match name {
                    ElementName::Public(key) => Ok(object
                        .has_property(key.to_property_key(context)?, context)?
                        .into()),
                    ElementName::Private(name) => Ok(object
                        .private_element_find(name, true, true)
                        .is_some()
                        .into()),
                }
            },
            name.clone(),
        ),
    )
    .name(js_string!("has"))
    .length(1)
    .build();
    access.create_data_property_or_throw(js_string!("has"), has, context)?;

    Ok(access)
}

/// Abstract operation `CreateDecoratorContextObject ( kind, name, initializers, decorationState [ , isStatic ] )`,
/// with the `metadata` of the decorator metadata proposal.
fn create_decorator_context_object(
    element: Option<&ElementRecord>,
    class_name: &JsString,
    initializers: &Initializers,
    finished: &Gc<GcRefCell<bool>>,
    metadata: Option<&JsObject>,
    context: &mut Context,
) -> JsResult<JsObject> {
    let object = JsObject::with_object_proto(context.intrinsics());
    if let Some(element) = element {
        object.create_data_property_or_throw(js_string!("kind"), element.kind.name(), context)?;
        let access = create_decorator_access_object(element.kind, &element.name, context)?;
        object.create_data_property_or_throw(js_string!("access"), access, context)?;
        object.create_data_property_or_throw(js_string!("static"), element.is_static, context)?;
        object.create_data_property_or_throw(
            js_string!("private"),
            matches!(element.name, ElementName::Private(_)),
            context,
        )?;
        object.create_data_property_or_throw(
            js_string!("name"),
            element.name.context_name(),
            context,
        )?;
    } else {
        object.create_data_property_or_throw(js_string!("kind"), js_string!("class"), context)?;
        object.create_data_property_or_throw(js_string!("name"), class_name.clone(), context)?;
    }
    let add_initializer = create_add_initializer_function(initializers, finished, context);
    object.create_data_property_or_throw(js_string!("addInitializer"), add_initializer, context)?;
    if let Some(metadata) = metadata {
        object.create_data_property_or_throw(js_string!("metadata"), metadata.clone(), context)?;
    }
    Ok(object)
}

/// Abstract operation `ApplyDecoratorsToElementDefinition ( homeObject, elementRecord, extraInitializers, isStatic )`
///
/// The decorators are applied innermost first, and an initializer returned by a later one runs
/// before those returned by earlier ones.
fn apply_decorators_to_element_definition(
    element: &mut ElementRecord,
    extra_initializers: &Initializers,
    metadata: Option<&JsObject>,
    context: &mut Context,
) -> JsResult<()> {
    let decorators = std::mem::take(&mut element.decorators);
    for decorator in decorators.iter().rev() {
        let finished = Gc::new(GcRefCell::new(false));
        let decorator_context = create_decorator_context_object(
            Some(element),
            &JsString::default(),
            extra_initializers,
            &finished,
            metadata,
            context,
        )?;
        let value = match element.kind {
            ElementKind::Method | ElementKind::Getter | ElementKind::Setter => element
                .value
                .clone()
                .map_or_else(JsValue::undefined, JsValue::from),
            ElementKind::Accessor => {
                let value = JsObject::with_object_proto(context.intrinsics());
                value.create_data_property_or_throw(
                    js_string!("get"),
                    element
                        .getter
                        .clone()
                        .map_or_else(JsValue::undefined, JsValue::from),
                    context,
                )?;
                value.create_data_property_or_throw(
                    js_string!("set"),
                    element
                        .setter
                        .clone()
                        .map_or_else(JsValue::undefined, JsValue::from),
                    context,
                )?;
                value.into()
            }
            ElementKind::Field | ElementKind::StaticBlock => JsValue::undefined(),
        };
        let function = decorator
            .function
            .as_callable()
            .ok_or_else(|| js_error!(TypeError: "decorator must be a function"))?;
        let new_value = function.call(
            &decorator.receiver,
            &[value, decorator_context.into()],
            context,
        )?;
        *finished.borrow_mut() = true;

        match element.kind {
            ElementKind::Field => {
                if let Some(initializer) = new_value.as_callable() {
                    element.initializers.insert(0, initializer);
                } else if !new_value.is_undefined() {
                    return Err(js_error!(
                        TypeError: "field decorator must return a function or undefined"
                    ));
                }
            }
            ElementKind::Accessor => {
                if let Some(object) = new_value.as_object() {
                    let getter = object.get(js_string!("get"), context)?;
                    if let Some(getter) = getter.as_callable() {
                        element.getter = Some(getter);
                    } else if !getter.is_undefined() {
                        return Err(
                            js_error!(TypeError: "accessor decorator get must be a function"),
                        );
                    }
                    let setter = object.get(js_string!("set"), context)?;
                    if let Some(setter) = setter.as_callable() {
                        element.setter = Some(setter);
                    } else if !setter.is_undefined() {
                        return Err(
                            js_error!(TypeError: "accessor decorator set must be a function"),
                        );
                    }
                    let initializer = object.get(js_string!("init"), context)?;
                    if let Some(initializer) = initializer.as_callable() {
                        element.initializers.insert(0, initializer);
                    } else if !initializer.is_undefined() {
                        return Err(
                            js_error!(TypeError: "accessor decorator init must be a function"),
                        );
                    }
                } else if !new_value.is_undefined() {
                    return Err(js_error!(
                        TypeError: "accessor decorator must return an object or undefined"
                    ));
                }
            }
            ElementKind::Method | ElementKind::Getter | ElementKind::Setter => {
                if let Some(function) = new_value.as_callable() {
                    element.value = Some(function);
                } else if !new_value.is_undefined() {
                    return Err(js_error!(
                        TypeError: "method decorator must return a function or undefined"
                    ));
                }
            }
            ElementKind::StaticBlock => {}
        }
    }
    Ok(())
}

/// Abstract operation `DefineMethodProperty ( homeObject, methodDefinition, enumerable )`
fn define_method_property(
    home: &JsObject,
    element: &ElementRecord,
    context: &mut Context,
) -> JsResult<()> {
    let ElementName::Public(key) = &element.name else {
        return Ok(());
    };
    let descriptor = PropertyDescriptor::builder()
        .enumerable(false)
        .configurable(true);
    let descriptor = match element.kind {
        ElementKind::Method => descriptor
            .value(
                element
                    .value
                    .clone()
                    .map_or_else(JsValue::undefined, JsValue::from),
            )
            .writable(true),
        ElementKind::Getter => descriptor.maybe_get(element.value.clone()),
        ElementKind::Setter => descriptor.maybe_set(element.value.clone()),
        ElementKind::Accessor => descriptor
            .maybe_get(element.getter.clone())
            .maybe_set(element.setter.clone()),
        ElementKind::Field | ElementKind::StaticBlock => return Ok(()),
    };
    home.define_property_or_throw(key.to_property_key(context)?, descriptor.build(), context)?;
    Ok(())
}

/// The private methods and accessors of the static or the non-static elements, with each
/// getter and setter pair as one accessor, as `InitializePrivateMethods` adds them.
fn private_methods(
    elements: &[ElementRecord],
    is_static: bool,
) -> Vec<(PrivateName, PrivateElement)> {
    let mut methods: Vec<(PrivateName, PrivateElement)> = Vec::new();
    for element in elements
        .iter()
        .filter(|element| element.is_static == is_static)
    {
        let ElementName::Private(name) = &element.name else {
            continue;
        };
        let (getter, setter) = match element.kind {
            ElementKind::Method => {
                if let Some(method) = element.value.clone() {
                    methods.push((name.clone(), PrivateElement::Method(method)));
                }
                continue;
            }
            ElementKind::Getter => (element.value.clone(), None),
            ElementKind::Setter => (None, element.value.clone()),
            ElementKind::Accessor => (element.getter.clone(), element.setter.clone()),
            ElementKind::Field | ElementKind::StaticBlock => continue,
        };
        if let Some((
            _,
            PrivateElement::Accessor {
                getter: existing_getter,
                setter: existing_setter,
            },
        )) = methods.iter_mut().find(|(key, _)| key == name)
        {
            if getter.is_some() {
                *existing_getter = getter;
            }
            if setter.is_some() {
                *existing_setter = setter;
            }
        } else {
            methods.push((name.clone(), PrivateElement::Accessor { getter, setter }));
        }
    }
    methods
}

/// `CreateClassDecorations` implements the Opcode Operation for `Opcode::CreateClassDecorations`
///
/// Operation:
///  - Starts the definition of a class with decorators or auto-accessors. `decorators` holds
///    the decorators of the class, or `undefined` if neither the class nor its elements have
///    any, in which case the class gets no metadata object.
#[derive(Debug, Clone, Copy)]
pub(crate) struct CreateClassDecorations;

impl CreateClassDecorations {
    #[inline(always)]
    pub(crate) fn operation(
        (dst, class, prototype, superclass, decorators, name): (
            RegisterOperand,
            RegisterOperand,
            RegisterOperand,
            RegisterOperand,
            RegisterOperand,
            IndexOperand,
        ),
        context: &mut Context,
    ) -> JsResult<()> {
        let class = context
            .vm
            .get_register(class.into())
            .as_object()
            .js_expect("class must be an object")?;
        let prototype = context
            .vm
            .get_register(prototype.into())
            .as_object()
            .js_expect("class prototype must be an object")?;
        let class_name = context.vm.frame().code_block().constant_string(name.into());
        let decorators = context.vm.get_register(decorators.into()).clone();
        let superclass = context.vm.get_register(superclass.into()).clone();

        let metadata = if decorators.is_undefined() {
            None
        } else {
            // Let metadataParent be ? Get(superclass, @@metadata), or null without a superclass.
            let parent = match superclass.as_object() {
                Some(superclass) => superclass.get(JsSymbol::metadata(), context)?.as_object(),
                None => None,
            };
            Some(JsObject::from_proto_and_data(parent, OrdinaryObject))
        };
        let decorators = read_decorators(&decorators, context)?;

        let state = JsObject::from_proto_and_data(
            None,
            ClassDecorations {
                class,
                prototype,
                class_name,
                decorators,
                metadata,
                elements: Vec::new(),
                static_method_initializers: Initializers::default(),
                instance_method_initializers: Initializers::default(),
                class_initializers: Initializers::default(),
            },
        );
        context.vm.set_register(dst.into(), state.into());
        Ok(())
    }
}

impl Operation for CreateClassDecorations {
    const NAME: &'static str = "CreateClassDecorations";
    const INSTRUCTION: &'static str = "INST - CreateClassDecorations";
    const COST: u8 = 4;
}

/// `AddClassElement` implements the Opcode Operation for `Opcode::AddClassElement`
///
/// Operation:
///  - Adds an element to a class started by `CreateClassDecorations`: `flags` are
///    [`ClassElementFlags`], `key` is its property key or private name description, `value`
///    its function and `decorators` an array of decorator and `this` value pairs, or
///    `undefined`.
#[derive(Debug, Clone, Copy)]
pub(crate) struct AddClassElement;

impl AddClassElement {
    #[inline(always)]
    pub(crate) fn operation(
        (state, flags, key, value, decorators): (
            RegisterOperand,
            IndexOperand,
            RegisterOperand,
            RegisterOperand,
            RegisterOperand,
        ),
        context: &mut Context,
    ) -> JsResult<()> {
        let flags = u32::from(flags);
        let kind = ElementKind::from_flags(flags);
        let is_static = flags & ClassElementFlags::STATIC != 0;
        let state = context
            .vm
            .get_register(state.into())
            .as_object()
            .js_expect("class decorations must be an object")?;
        let key = context.vm.get_register(key.into()).clone();
        let value = context
            .vm
            .get_register(value.into())
            .as_object()
            .js_expect("class element value must be a function object")?;
        let decorators = context.vm.get_register(decorators.into()).clone();
        let decorators = read_decorators(&decorators, context)?;

        let (class, prototype) = {
            let state = state
                .downcast_ref::<ClassDecorations>()
                .js_expect("class decorations must be a ClassDecorations object")?;
            (state.class.clone(), state.prototype.clone())
        };
        let home = if is_static { &class } else { &prototype };
        let name = if flags & ClassElementFlags::PRIVATE != 0 {
            ElementName::Private(
                class.private_name(key.as_string().js_expect("private name must be a string")?),
            )
        } else {
            ElementName::Public(key)
        };

        let mut element = ElementRecord {
            kind,
            is_static,
            name,
            value: Some(value.clone()),
            getter: None,
            setter: None,
            storage: None,
            initializers: Vec::new(),
            extra_initializers: Initializers::default(),
            decorators,
            names_anonymous_function: flags & ClassElementFlags::ANONYMOUS_FUNCTION != 0,
        };
        match kind {
            ElementKind::Method | ElementKind::Getter | ElementKind::Setter => {
                set_home_object(&value, home)?;
                let prefix = match kind {
                    ElementKind::Getter => Some(js_str!("get")),
                    ElementKind::Setter => Some(js_str!("set")),
                    _ => None,
                };
                let key = element.name.function_key(context)?;
                set_function_name(&value, &key, prefix, context)?;
            }
            ElementKind::Field => set_home_object(&value, home)?,
            ElementKind::Accessor => {
                set_home_object(&value, home)?;
                let readable = match &element.name {
                    ElementName::Private(name) => js_string!(js_str!("#"), name.description()),
                    ElementName::Public(key) => match key.as_symbol() {
                        Some(symbol) => symbol.descriptive_string(),
                        None => key.to_string(context)?,
                    },
                };
                let storage =
                    PrivateName::unique(js_string!(&readable, js_str!(" accessor storage")));
                let (getter, setter) = make_auto_accessor(&element.name, &storage, context)?;
                element.getter = Some(getter);
                element.setter = Some(setter);
                element.storage = Some(storage);
            }
            ElementKind::StaticBlock => set_home_object(&value, &class)?,
        }

        state
            .downcast_mut::<ClassDecorations>()
            .js_expect("class decorations must be a ClassDecorations object")?
            .elements
            .push(element);
        Ok(())
    }
}

impl Operation for AddClassElement {
    const NAME: &'static str = "AddClassElement";
    const INSTRUCTION: &'static str = "INST - AddClassElement";
    const COST: u8 = 6;
}

/// `DecorateClass` implements the Opcode Operation for `Opcode::DecorateClass`
///
/// Operation:
///  - Applies the decorators of the elements, defines the elements, applies the decorators of
///    the class and stores the class they return in `dst`.
#[derive(Debug, Clone, Copy)]
pub(crate) struct DecorateClass;

impl DecorateClass {
    #[inline(always)]
    pub(crate) fn operation(
        (state, dst): (RegisterOperand, RegisterOperand),
        context: &mut Context,
    ) -> JsResult<()> {
        let state = context
            .vm
            .get_register(state.into())
            .as_object()
            .js_expect("class decorations must be an object")?;
        let (
            class,
            prototype,
            class_name,
            decorators,
            metadata,
            mut elements,
            static_method_initializers,
            instance_method_initializers,
            class_initializers,
        ) = {
            let mut state = state
                .downcast_mut::<ClassDecorations>()
                .js_expect("class decorations must be a ClassDecorations object")?;
            (
                state.class.clone(),
                state.prototype.clone(),
                state.class_name.clone(),
                std::mem::take(&mut state.decorators),
                state.metadata.clone(),
                std::mem::take(&mut state.elements),
                state.static_method_initializers.clone(),
                state.instance_method_initializers.clone(),
                state.class_initializers.clone(),
            )
        };

        // Apply the decorators of the static methods, getters, setters and auto-accessors and
        // define them, then those of the non-static ones, then those of the static fields and
        // then those of the non-static fields.
        for is_static in [true, false] {
            let (home, method_initializers) = if is_static {
                (&class, &static_method_initializers)
            } else {
                (&prototype, &instance_method_initializers)
            };
            for element in elements.iter_mut().filter(|element| {
                element.is_static == is_static
                    && !matches!(element.kind, ElementKind::Field | ElementKind::StaticBlock)
            }) {
                let extra_initializers = if element.kind == ElementKind::Accessor {
                    element.extra_initializers.clone()
                } else {
                    method_initializers.clone()
                };
                apply_decorators_to_element_definition(
                    element,
                    &extra_initializers,
                    metadata.as_ref(),
                    context,
                )?;
                define_method_property(home, element, context)?;
            }
        }
        for is_static in [true, false] {
            for element in elements.iter_mut().filter(|element| {
                element.is_static == is_static && element.kind == ElementKind::Field
            }) {
                let extra_initializers = element.extra_initializers.clone();
                apply_decorators_to_element_definition(
                    element,
                    &extra_initializers,
                    metadata.as_ref(),
                    context,
                )?;
            }
        }

        // Set F.[[Elements]] to instanceElements and F.[[Initializers]] to
        // instanceMethodExtraInitializers; the initializers run before the fields.
        {
            let mut function = class
                .downcast_mut::<OrdinaryFunction>()
                .js_expect("class must be a function object")?;
            for initializer in instance_method_initializers.borrow().iter() {
                function
                    .push_field_definition(ClassFieldDefinition::Initializer(initializer.clone()));
            }
            for element in elements
                .iter()
                .filter(|element| !element.is_static && element.kind.is_field_or_accessor())
            {
                function.push_field_definition(ClassFieldDefinition::Decorated(Gc::new(
                    element.field(),
                )));
            }
            for (name, method) in private_methods(&elements, false) {
                function.push_private_method(name, method);
            }
        }

        // Perform ? InitializePrivateMethods(F, staticElements).
        for (name, method) in private_methods(&elements, true) {
            class.private_method_or_accessor_add(&name, &method, context)?;
        }

        // Let newF be ? ApplyDecoratorsToClassDefinition(F, decorators, className, classExtraInitializers).
        let mut decorated = class;
        for decorator in decorators.iter().rev() {
            let finished = Gc::new(GcRefCell::new(false));
            let decorator_context = create_decorator_context_object(
                None,
                &class_name,
                &class_initializers,
                &finished,
                metadata.as_ref(),
                context,
            )?;
            let function = decorator
                .function
                .as_callable()
                .ok_or_else(|| js_error!(TypeError: "decorator must be a function"))?;
            let new_class = function.call(
                &decorator.receiver,
                &[decorated.clone().into(), decorator_context.into()],
                context,
            )?;
            *finished.borrow_mut() = true;
            if let Some(new_class) = new_class.as_callable() {
                decorated = new_class;
            } else if !new_class.is_undefined() {
                return Err(js_error!(
                    TypeError: "class decorator must return a function or undefined"
                ));
            }
        }

        // The decorator metadata proposal: the class's metadata object is its @@metadata.
        if let Some(metadata) = metadata {
            decorated.define_property_or_throw(
                JsSymbol::metadata(),
                PropertyDescriptor::builder()
                    .value(metadata)
                    .writable(false)
                    .enumerable(false)
                    .configurable(true)
                    .build(),
                context,
            )?;
        }

        {
            let mut state = state
                .downcast_mut::<ClassDecorations>()
                .js_expect("class decorations must be a ClassDecorations object")?;
            state.class = decorated.clone();
            state.elements = elements
                .into_iter()
                .filter(|element| {
                    element.is_static
                        && (element.kind.is_field_or_accessor()
                            || element.kind == ElementKind::StaticBlock)
                })
                .collect();
        }
        context.vm.set_register(dst.into(), decorated.into());
        Ok(())
    }
}

impl Operation for DecorateClass {
    const NAME: &'static str = "DecorateClass";
    const INSTRUCTION: &'static str = "INST - DecorateClass";
    const COST: u8 = 6;
}

/// `InitializeStaticElements` implements the Opcode Operation for `Opcode::InitializeStaticElements`
///
/// Operation:
///  - Runs the initializers that the decorators of static methods added, the static fields,
///    auto-accessors and blocks in order, and the initializers that class decorators added.
#[derive(Debug, Clone, Copy)]
pub(crate) struct InitializeStaticElements;

impl InitializeStaticElements {
    #[inline(always)]
    pub(crate) fn operation(state: RegisterOperand, context: &mut Context) -> JsResult<()> {
        let state = context
            .vm
            .get_register(state.into())
            .as_object()
            .js_expect("class decorations must be an object")?;
        let (class, elements, static_method_initializers, class_initializers) = {
            let mut state = state
                .downcast_mut::<ClassDecorations>()
                .js_expect("class decorations must be a ClassDecorations object")?;
            (
                state.class.clone(),
                std::mem::take(&mut state.elements),
                state.static_method_initializers.borrow().clone(),
                state.class_initializers.borrow().clone(),
            )
        };
        let this = JsValue::from(class.clone());

        for initializer in static_method_initializers {
            initializer.call(&this, &[], context)?;
        }
        for element in elements {
            if element.kind == ElementKind::StaticBlock {
                if let Some(body) = &element.value {
                    body.call(&this, &[], context)?;
                }
            } else {
                element.field().initialize(&class, context)?;
            }
        }
        for initializer in class_initializers {
            initializer.call(&this, &[], context)?;
        }
        Ok(())
    }
}

impl Operation for InitializeStaticElements {
    const NAME: &'static str = "InitializeStaticElements";
    const INSTRUCTION: &'static str = "INST - InitializeStaticElements";
    const COST: u8 = 6;
}
