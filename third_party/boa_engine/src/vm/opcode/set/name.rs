use boa_ast::scope::{BindingLocator, BindingLocatorScope};

use crate::{
    Context, JsError, JsExpect, JsNativeError, JsResult, JsValue,
    environments::Environment,
    object::{internal_methods::InternalMethodPropertyContext, shape::slot::SlotAttributes},
    property::PropertyKey,
    vm::opcode::{IndexOperand, Operation, RegisterOperand},
};

/// `ThrowMutateImmutable` implements the Opcode Operation for `Opcode::ThrowMutateImmutable`
///
/// Operation:
///  - Throws an error because the binding access is illegal.
#[derive(Debug, Clone, Copy)]
pub(crate) struct ThrowMutateImmutable;

impl ThrowMutateImmutable {
    #[inline(always)]
    pub(crate) fn operation(index: IndexOperand, context: &mut Context) -> JsError {
        let name = context
            .vm
            .frame()
            .code_block()
            .constant_string(index.into());

        JsNativeError::typ()
            .with_message(format!(
                "cannot mutate an immutable binding '{}'",
                name.to_std_string_escaped()
            ))
            .into()
    }
}

impl Operation for ThrowMutateImmutable {
    const NAME: &'static str = "ThrowMutateImmutable";
    const INSTRUCTION: &'static str = "INST - ThrowMutateImmutable";
    const COST: u8 = 2;
}

/// `SetName` implements the Opcode Operation for `Opcode::SetName`
///
/// Operation:
///  - Find a binding on the environment chain and assign its value.
#[derive(Debug, Clone, Copy)]
pub(crate) struct SetName;

impl SetName {
    #[inline(always)]
    pub(crate) fn operation(
        (value, index): (RegisterOperand, IndexOperand),
        context: &mut Context,
    ) -> JsResult<()> {
        let value = context.vm.get_register(value.into()).clone();
        let code_block = context.vm.frame().code_block();
        let mut binding_locator = code_block.bindings[usize::from(index)].clone();
        let strict = context.vm.frame().strict();

        context.find_runtime_binding(&mut binding_locator)?;

        verify_initialized(&binding_locator, context)?;

        context.set_binding(&binding_locator, value.clone(), strict)?;

        Ok(())
    }
}

impl Operation for SetName {
    const NAME: &'static str = "SetName";
    const INSTRUCTION: &'static str = "INST - SetName";
    const COST: u8 = 4;
}

/// `SetNameByLocator` implements the Opcode Operation for `Opcode::SetNameByLocator`
///
/// Operation:
///  - Assigns a value to the binding pointed by the `current_binding` of the current frame.
#[derive(Debug, Clone, Copy)]
pub(crate) struct SetNameByLocator;

impl SetNameByLocator {
    #[inline(always)]
    pub(crate) fn operation(value: RegisterOperand, context: &mut Context) -> JsResult<()> {
        let frame = context.vm.frame_mut();
        let strict = frame.strict();
        let (mut binding_locator, unresolvable) = frame
            .binding_stack
            .pop()
            .js_expect("locator should have been popped before")?;
        let value = context.vm.get_register(value.into()).clone();

        if unresolvable {
            return Err(JsNativeError::reference()
                .with_message(format!(
                    "{} is not defined",
                    binding_locator.name().to_std_string_escaped()
                ))
                .into());
        }

        if context.is_deleted_binding(&binding_locator) {
            context.find_runtime_binding(&mut binding_locator)?;
        }

        verify_initialized(&binding_locator, context)?;

        context.set_binding(&binding_locator, value.clone(), strict)?;

        Ok(())
    }
}

impl Operation for SetNameByLocator {
    const NAME: &'static str = "SetNameByLocator";
    const INSTRUCTION: &'static str = "INST - SetNameByLocator";
    const COST: u8 = 4;
}

/// `SetNameGlobal` implements the Opcode Operation for `Opcode::SetNameGlobal`
///
/// Operation:
///  - Like `SetName`, writing a global object binding through an inline cache.
#[derive(Debug, Clone, Copy)]
pub(crate) struct SetNameGlobal;

impl SetNameGlobal {
    #[inline(always)]
    pub(crate) fn operation(
        (value, index, ic_index): (RegisterOperand, IndexOperand, IndexOperand),
        context: &mut Context,
    ) -> JsResult<()> {
        let value = context.vm.get_register(value.into()).clone();
        let code_block = context.vm.frame().code_block();
        let mut binding_locator = code_block.bindings[usize::from(index)].clone();
        let strict = context.vm.frame().strict();

        context.find_runtime_binding(&mut binding_locator)?;

        if binding_locator.is_global() {
            return set_global_object_binding(&binding_locator, value, ic_index, true, context);
        }

        verify_initialized(&binding_locator, context)?;

        context.set_binding(&binding_locator, value, strict)?;

        Ok(())
    }
}

impl Operation for SetNameGlobal {
    const NAME: &'static str = "SetNameGlobal";
    const INSTRUCTION: &'static str = "INST - SetNameGlobal";
    const COST: u8 = 4;
}

/// `SetNameByLocatorGlobal` implements the Opcode Operation for `Opcode::SetNameByLocatorGlobal`
///
/// Operation:
///  - Like `SetNameByLocator`, writing a global object binding through an inline cache.
#[derive(Debug, Clone, Copy)]
pub(crate) struct SetNameByLocatorGlobal;

impl SetNameByLocatorGlobal {
    #[inline(always)]
    pub(crate) fn operation(
        (value, ic_index): (RegisterOperand, IndexOperand),
        context: &mut Context,
    ) -> JsResult<()> {
        let frame = context.vm.frame_mut();
        let strict = frame.strict();
        let (mut binding_locator, unresolvable) = frame
            .binding_stack
            .pop()
            .js_expect("locator should have been popped before")?;
        let value = context.vm.get_register(value.into()).clone();

        if unresolvable {
            return Err(JsNativeError::reference()
                .with_message(format!(
                    "{} is not defined",
                    binding_locator.name().to_std_string_escaped()
                ))
                .into());
        }

        if context.is_deleted_binding(&binding_locator) {
            context.find_runtime_binding(&mut binding_locator)?;
        }

        if binding_locator.is_global() {
            return set_global_object_binding(&binding_locator, value, ic_index, true, context);
        }

        verify_initialized(&binding_locator, context)?;

        context.set_binding(&binding_locator, value, strict)?;

        Ok(())
    }
}

impl Operation for SetNameByLocatorGlobal {
    const NAME: &'static str = "SetNameByLocatorGlobal";
    const INSTRUCTION: &'static str = "INST - SetNameByLocatorGlobal";
    const COST: u8 = 4;
}

/// Writes `value` to the global object property a global binding names, through the inline
/// cache at `ic_index`.
///
/// A cached own data property is written directly; otherwise strict code first checks that the
/// property still exists (when `check_exists` is set) and the value is stored with `[[Set]]`.
pub(crate) fn set_global_object_binding(
    binding_locator: &BindingLocator,
    value: JsValue,
    ic_index: IndexOperand,
    check_exists: bool,
    context: &mut Context,
) -> JsResult<()> {
    let object = context.global_object();
    let strict = context.vm.frame().strict();

    let ic = &context.vm.frame().code_block().ic[usize::from(ic_index)];
    let object_borrowed = object.borrow();
    if let Some((_, slot)) = ic.get(object_borrowed.shape())
        && !slot.attributes.is_accessor_descriptor()
        && !slot.attributes.contains(SlotAttributes::PROTOTYPE)
    {
        drop(object_borrowed);
        object.borrow_mut().properties_mut().storage[slot.index as usize] = value;
        return Ok(());
    }
    let looked_up = object_borrowed.shape().clone();
    drop(object_borrowed);

    let key: PropertyKey = ic.name.clone().into();

    if check_exists && strict && !object.has_property(key.clone(), context)? {
        return Err(JsNativeError::reference()
            .with_message(format!(
                "cannot assign to uninitialized global property `{}`",
                binding_locator.name().to_std_string_escaped()
            ))
            .into());
    }

    let context = &mut InternalMethodPropertyContext::new(context);
    let succeeded = object.__set__(key.clone(), value, object.clone().into(), context)?;
    if !succeeded {
        if strict {
            return Err(JsNativeError::typ()
                .with_message(format!("cannot set non-writable property: {key}"))
                .into());
        }
        return Ok(());
    }

    // Cache the property.
    let slot = *context.slot();
    if slot.is_cacheable() {
        let ic = &context.vm.frame().code_block.ic[usize::from(ic_index)];
        let object_borrowed = object.borrow();
        ic.set_after_access(&looked_up, object_borrowed.shape(), slot);
    }

    Ok(())
}

/// Checks that the binding pointed by `locator` exists and is initialized.
fn verify_initialized(locator: &BindingLocator, context: &mut Context) -> JsResult<()> {
    if !context.is_initialized_binding(locator)? {
        let key = locator.name();
        let strict = context.vm.frame().strict();

        let message = match locator.scope() {
            BindingLocatorScope::GlobalObject if strict => Some(format!(
                "cannot assign to uninitialized global property `{}`",
                key.to_std_string_escaped()
            )),
            BindingLocatorScope::GlobalObject => None,
            BindingLocatorScope::GlobalDeclarative => Some(format!(
                "cannot assign to uninitialized binding `{}`",
                key.to_std_string_escaped()
            )),
            BindingLocatorScope::Stack(index) => match context.environment_expect(index) {
                Environment::Declarative(_) => Some(format!(
                    "cannot assign to uninitialized binding `{}`",
                    key.to_std_string_escaped()
                )),
                Environment::Object(_) if strict => Some(format!(
                    "cannot assign to uninitialized property `{}`",
                    key.to_std_string_escaped()
                )),
                Environment::Object(_) => None,
            },
        };

        if let Some(message) = message {
            return Err(JsNativeError::reference().with_message(message).into());
        }
    }

    Ok(())
}
